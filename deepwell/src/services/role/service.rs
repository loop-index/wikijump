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
use crate::error::{Error, ErrorType};
use crate::models::permission::Model as PermissionModel;
use crate::models::role::{self, Entity as Role, Model as RoleModel};
use crate::models::user_role;
use crate::models::user_role::Model as UserRoleModel;
use crate::services::ServiceContext;
use crate::services::audit::{AuditEvent, AuditService};
use crate::services::permission::PermissionService;
use crate::services::role::GUEST_ROLE_NAME;
use crate::utils::now;
use sea_orm::prelude::TimeDateTimeWithTimeZone;
use std::net::IpAddr;

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
        reference: Reference<'_>,
        UpdateRoleInput {
            name,
            description,
            level,
            permission_ids,
        }: UpdateRoleInput,
        updating_user_id: i64,
        ip_address: IpAddr,
    ) -> Result<RoleModel> {
        let txn = ctx.transaction();

        let role = Self::get(ctx, reference)
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
        let current_permissions =
            PermissionService::get_permission_ids_for_role(ctx, role.role_id)
                .await
                .or_raise(make_error)?;

        // TODO: Make this more efficient
        for permission_id in &current_permissions {
            PermissionService::remove_permission_from_role(
                ctx,
                role.role_id,
                *permission_id,
            )
            .await
            .or_raise(make_error)?;
        }

        let new_permissions = if let Maybe::Set(permission_ids) = &permission_ids {
            for permission_id in permission_ids {
                PermissionService::add_permission_to_role(
                    ctx,
                    role.role_id,
                    *permission_id,
                )
                .await
                .or_raise(make_error)?;
            }
            permission_ids.clone()
        } else {
            Vec::new()
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
        reference: Reference<'_>,
        deleting_user_id: i64,
        ip_address: IpAddr,
    ) -> Result<()> {
        let txn = ctx.transaction();

        let role = Self::get(ctx, reference)
            .await
            .or_raise(|| Error::new("failed to delete role", ErrorType::Role))?;

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
        reference: Reference<'_>,
    ) -> Result<Option<RoleModel>> {
        let txn = ctx.transaction();

        let make_error = || Error::new("failed to get role", ErrorType::Role);

        let role = match reference {
            Reference::Id(id) => {
                Role::find_by_id(id).one(txn).await.or_raise(make_error)?
            }
            _ => None,
        };

        Ok(role)
    }

    #[inline]
    pub async fn get(
        ctx: &ServiceContext<'_>,
        reference: Reference<'_>,
    ) -> Result<RoleModel> {
        find_or_error!(Self::get_optional(ctx, reference), "role", Role)
    }

    pub async fn grant_role_to_user(
        ctx: &ServiceContext<'_>,
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

        let role = Self::get(ctx, role_id.into()).await?;

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

    pub async fn get_guest_role_for_site(
        ctx: &ServiceContext<'_>,
        site_id: i64,
    ) -> Result<RoleModel> {
        let txn = ctx.transaction();

        let make_error = || Error::new("failed to get guest role", ErrorType::Role);

        let role = Role::find()
            .filter(
                role::Column::SiteId
                    .eq(site_id)
                    .and(role::Column::Name.eq(GUEST_ROLE_NAME)),
            )
            .one(txn)
            .await
            .or_raise(make_error)?;

        Ok(role.unwrap())
    }
}
