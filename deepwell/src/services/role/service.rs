/*
 * services/role/service.rs
 *
 * DEEPWELL - Wikijump API provider and database manager
 * Copyright (C) 2019-2026 Wikijump Team
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <http://www.gnu.org/licenses/>.
 */
use super::prelude::*;
use crate::endpoints::user;
use crate::error::{Error, ErrorType};
use crate::models::prelude::Page;
use crate::models::role::{self, Entity as Role, Model as RoleModel};
use crate::models::role_permission::{self, Entity as RolePermission};
use crate::models::user_role::{Entity as UserRole, Model as UserRoleModel};
use crate::models::{page, user_role};
use crate::services::audit::{AuditEvent, AuditService};
use crate::services::permission::{CheckPermissionContext, PermissionService};
use crate::services::relation::{GetPageAttributions, GetSiteMember, SiteMemberAccepted};
use crate::services::role::SystemRole;
use crate::services::{PageService, RelationService, ServiceContext};
use crate::types::{Action, Permission, Reference, Resource};
use crate::utils::{now, trim_default};
use sea_orm::prelude::Expr;
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug)]
pub struct RoleService;

#[allow(dead_code)] // Temp
impl RoleService {
    pub async fn create(
        ctx: &ServiceContext<'_>,
        CreateRoleInput {
            site_id,
            name,
            description,
            is_virtual,
            is_system,
            level,
        }: CreateRoleInput,
        ip_address: IpAddr,
    ) -> Result<CreateRoleOutput> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!("failed to create role '{}' in site ID {}", name, site_id,),
                ErrorType::Role,
            )
        };

        // Insert role
        let now = now();

        let model = role::ActiveModel {
            site_id: Set(site_id),
            name: Set(name.clone()),
            description: Set(description.clone().unwrap_or_default()),
            is_virtual: Set(is_virtual),
            is_system: Set(is_system),
            level: Set(level),
            created_at: Set(now),
            updated_at: Set(Some(now)),
            ..Default::default()
        };

        let RoleModel { role_id, .. } = model.insert(txn).await.or_raise(make_error)?;

        AuditService::log(ctx, ip_address, AuditEvent::RoleCreate { site_id, role_id })
            .await
            .or_raise(make_error)?;

        Ok(CreateRoleOutput { role_id })
    }

    pub async fn update(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
        UpdateRoleInput {
            name,
            description,
            level,
            permissions,
        }: UpdateRoleInput,
        updating_user_id: i64,
        ip_address: IpAddr,
    ) -> Result<RoleModel> {
        let txn = ctx.transaction();

        let role = Self::get(ctx, site_id, reference)
            .await
            .or_raise(|| Error::new("failed to update role data", ErrorType::Role))?;

        let mut model = role::ActiveModel {
            role_id: Set(role.role_id),
            updated_at: Set(Some(now())),
            ..Default::default()
        };

        let make_error = || {
            Error::new(
                format!(
                    "failed to update role ID {}, changed by user ID {}",
                    role.role_id, updating_user_id,
                ),
                ErrorType::Role,
            )
        };

        // Update fields
        if let Maybe::Set(name_val) = &name {
            model.name = Set(name_val.clone());
        }

        if let Maybe::Set(description_val) = &description {
            model.description = Set(description_val.clone());
        }

        if let Maybe::Set(level_val) = level {
            model.level = Set(level_val);
        }

        // Update permissions
        let current_permissions: Vec<Permission> =
            PermissionService::get_permissions_for_role(ctx, role.role_id)
                .await
                .or_raise(make_error)?
                .into_iter()
                .filter_map(|perm| {
                    Some(Permission {
                        resource: Resource::from_str(&perm.resource_type).ok()?,
                        resource_category: perm.resource_category_id.map(Reference::Id),
                        action: Action::from_str(&perm.action).ok()?,
                    })
                })
                .collect();

        // TODO: Make this more efficient

        // Remove all existing permissions for this role
        RolePermission::delete_many()
            .filter(role_permission::Column::RoleId.eq(role.role_id))
            .exec(txn)
            .await
            .or_raise(make_error)?;

        // Add new permissions for this role
        let new_permissions = match &permissions {
            Maybe::Set(permissions) => {
                let models: Vec<role_permission::ActiveModel> = permissions
                    .iter()
                    .map(|permission| role_permission::ActiveModel {
                        role_id: Set(role.role_id),
                        site_id: Set(role.site_id),
                        resource_type: Set(permission.resource.to_string()),
                        resource_category_id: Set(permission
                            .resource_category
                            .as_ref()
                            .and_then(|r| match r {
                                Reference::Id(id) => Some(*id),
                                Reference::Slug(_) => None,
                            })),
                        action: Set(permission.action.to_string()),
                        ..Default::default()
                    })
                    .collect();

                RolePermission::insert_many(models)
                    .exec(txn)
                    .await
                    .or_raise(make_error)?;

                permissions.clone()
            }
            _ => Vec::new(),
        };

        let updated_role = model.update(txn).await.or_raise(make_error)?;

        AuditService::log(
            ctx,
            ip_address,
            AuditEvent::RoleUpdate {
                role_id: role.role_id,
                updating_user_id,
                level: updated_role.level,
                old_permissions: current_permissions,
                new_permissions,
            },
        )
        .await
        .or_raise(make_error)?;

        Ok(updated_role)
    }

    pub async fn delete(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
        deleting_user_id: i64,
        ip_address: IpAddr,
    ) -> Result<()> {
        let txn = ctx.transaction();

        let role = Self::get(ctx, site_id, reference)
            .await
            .or_raise(|| Error::new("failed to delete role", ErrorType::Role))?;

        // Remove this role from all users who actively have it
        UserRole::update_many()
            .col_expr(user_role::Column::DeletedAt, Expr::value(Some(now())))
            .filter(
                user_role::Column::RoleId
                    .eq(role.role_id)
                    .and(user_role::Column::DeletedAt.is_null()),
            )
            .exec(txn)
            .await
            .or_raise(|| {
                Error::new("failed to remove role from users", ErrorType::Role)
            })?;

        let make_error = || {
            Error::new(
                format!(
                    "failed to delete role ID {} by user ID {}",
                    role.role_id, deleting_user_id,
                ),
                ErrorType::Role,
            )
        };

        role::ActiveModel {
            role_id: Set(role.role_id),
            deleted_at: Set(Some(now())),
            ..Default::default()
        }
        .update(txn)
        .await
        .or_raise(make_error)?;

        AuditService::log(
            ctx,
            ip_address,
            AuditEvent::RoleDelete {
                role_id: role.role_id,
                deleting_user_id,
            },
        )
        .await
        .or_raise(make_error)?;

        Ok(())
    }

    pub async fn get_optional(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
    ) -> Result<Option<RoleModel>> {
        let txn = ctx.transaction();

        let make_error = || Error::new("failed to get role", ErrorType::Role);

        let role = {
            let condition = match reference {
                Reference::Id(id) => role::Column::RoleId.eq(id),
                Reference::Slug(slug) => {
                    // Get role by role name
                    role::Column::Name
                        .eq(slug)
                        .and(role::Column::DeletedAt.is_null())
                }
            };

            Role::find()
                .filter(
                    Condition::all()
                        .add(condition)
                        .add(role::Column::SiteId.eq(site_id)),
                )
                .one(txn)
                .await
                .or_raise(make_error)?
        };

        Ok(role)
    }

    #[inline]
    pub async fn get(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
    ) -> Result<RoleModel> {
        find_or_error!(Self::get_optional(ctx, site_id, reference), "role", Role)
    }

    pub async fn grant_role_to_user(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        GrantUserRoleInput {
            user_id,
            role_id,
            assigning_user_id,
            expires_at,
        }: GrantUserRoleInput,
        ip_address: IpAddr,
    ) -> Result<UserRoleModel> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!("failed to grant role ID {} to user ID {}", role_id, user_id,),
                ErrorType::GrantUserRole,
            )
        };

        let role = Self::get(ctx, site_id, role_id.into()).await?;

        let user_role = user_role::ActiveModel {
            user_id: Set(user_id),
            role_id: Set(role.role_id),
            site_id: Set(role.site_id),
            assigned_at: Set(now()),
            assigned_by: Set(assigning_user_id),
            expires_at: Set(expires_at),
            ..Default::default()
        }
        .insert(txn)
        .await
        .or_raise(make_error)?;

        AuditService::log(
            ctx,
            ip_address,
            AuditEvent::GrantUserRole {
                user_id,
                role_id,
                assigning_user_id,
                expires_at,
            },
        )
        .await
        .or_raise(make_error)?;

        Ok(user_role)
    }

    pub async fn revoke_role_from_user(
        ctx: &ServiceContext<'_>,
        user_id: i64,
        role_id: i64,
        revoking_user_id: i64,
        ip_address: IpAddr,
    ) -> Result<UserRoleModel> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!(
                    "failed to revoke role ID {} from user ID {}",
                    role_id, user_id,
                ),
                ErrorType::RevokeUserRole,
            )
        };

        let _user_role = user_role::Entity::find()
            .filter(
                user_role::Column::UserId
                    .eq(user_id)
                    .and(user_role::Column::RoleId.eq(role_id)),
            )
            .one(txn)
            .await
            .or_raise(make_error)?;

        let deleted_user_role = user_role::ActiveModel {
            user_id: Set(user_id),
            role_id: Set(role_id),
            deleted_at: Set(Option::from(now())),
            ..Default::default()
        }
        .update(txn)
        .await
        .or_raise(make_error)?;

        AuditService::log(
            ctx,
            ip_address,
            AuditEvent::RevokeUserRole {
                user_id,
                role_id,
                revoking_user_id,
            },
        )
        .await
        .or_raise(make_error)?;

        Ok(deleted_user_role)
    }

    pub async fn get_all_roles_for_user_and_site(
        ctx: &ServiceContext<'_>,
        input: GetUserRolesInput<'_>,
    ) -> Result<Vec<RoleModel>> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!(
                    "failed to get roles for user ID {} in site ID {}",
                    input.user_id.unwrap_or(-1),
                    input.site_id
                ),
                ErrorType::Role,
            )
        };

        let mut roles = match input.user_id {
            Some(id) => Role::find()
                .join(JoinType::InnerJoin, role::Relation::UserRole.def())
                .filter(
                    user_role::Column::UserId
                        .eq(id)
                        .and(role::Column::SiteId.eq(input.site_id))
                        .and(role::Column::DeletedAt.is_null()),
                )
                .all(txn)
                .await
                .or_raise(make_error)?,
            None => Vec::new(),
        };

        let virtual_roles = Self::get_virtual_roles_for_user(ctx, &input)
            .await
            .or_raise(make_error)?;

        roles.extend(virtual_roles);

        info!(
            "User ID {:?} has these roles in site ID {}: {:?}",
            input.user_id,
            input.site_id,
            roles.iter().map(|r| &r.name).collect::<Vec<_>>()
        );

        Ok(roles)
    }

    pub async fn get_virtual_roles_for_user(
        ctx: &ServiceContext<'_>,
        input: &GetUserRolesInput<'_>,
    ) -> Result<Vec<RoleModel>> {
        let txn = ctx.transaction();

        let make_error = || Error::new("failed to apply virtual roles", ErrorType::Role);

        let virtual_roles = Role::find()
            .filter(
                Condition::all()
                    .add(role::Column::SiteId.eq(input.site_id))
                    .add(role::Column::IsVirtual.eq(true)), // Virtual roles are never deleted, so we don't need to check DeletedAt
            )
            .all(txn)
            .await
            .or_raise(make_error)?;

        let virtual_role_name_map = virtual_roles
            .into_iter()
            .map(|role| (role.name.clone(), role))
            .collect::<std::collections::HashMap<_, _>>();

        // Compute user state flags
        let is_logged_in = input.user_id.is_some();
        let is_member = if is_logged_in {
            let membership = RelationService::get_optional_site_member(
                ctx,
                GetSiteMember {
                    site_id: input.site_id,
                    user_id: input.user_id.unwrap(),
                },
            )
            .await
            .or_raise(make_error)?;

            // TODO: Add invitation acceptance logic here
            membership.is_some()
        } else {
            false
        };
        let is_page_author = if is_member && let Some(page_ref) = &input.page_reference {
            let attributions = RelationService::get_page_attributions(
                ctx,
                GetPageAttributions {
                    site_id: input.site_id,
                    page: page_ref.clone(),
                },
            )
            .await
            .or_raise(make_error)?;
            attributions
                .iter()
                .any(|attr| attr.user_id == input.user_id.unwrap())
        } else {
            false
        };

        // Collect role names to add based on flags
        let mut applied_virtual_roles = Vec::new();
        if is_logged_in {
            applied_virtual_roles.push(SystemRole::Registered.to_string());
        }
        if is_member {
            applied_virtual_roles.push(SystemRole::Member.to_string());
        }
        if is_page_author {
            applied_virtual_roles.push(SystemRole::PageAuthor.to_string());
        }
        if !is_logged_in {
            applied_virtual_roles.push(SystemRole::Anonymous.to_string());
            applied_virtual_roles.push(SystemRole::Guest.to_string());
        } else if !is_member {
            applied_virtual_roles.push(SystemRole::Guest.to_string());
        }
        applied_virtual_roles.push(SystemRole::Everyone.to_string());

        info!(
            "Applying these virtual roles for user ID {:?} in site ID {}: {:?}",
            input.user_id, input.site_id, applied_virtual_roles
        );

        // Build the final list of applicable roles
        let applicable_virtual_roles = applied_virtual_roles
            .into_iter()
            .filter_map(|name| virtual_role_name_map.get(&name).cloned())
            .collect();

        Ok(applicable_virtual_roles)
    }
}
