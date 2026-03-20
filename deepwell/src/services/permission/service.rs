/*
 * services/permission/service.rs
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
use crate::endpoints::site;
use crate::error::{Error, ErrorType};
use crate::models::prelude::{Role, RolePermission};
use crate::models::role_permission;
use crate::models::role_permission::Model as RolePermissionModel;
use crate::models::{role, user_role};
use crate::services::ServiceContext;
use crate::services::permission::{
    CheckPermissionContext, PermissionInput, resolve_category_reference,
};
use crate::services::role::{GetUserRolesInput, RoleService};
use crate::types::{Action, Reference, Resource};

#[derive(Debug)]
pub struct PermissionService;

#[allow(dead_code)] // TEMP
impl PermissionService {
    pub async fn add_permission_to_role(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        role_id: i64,
        PermissionInput {
            resource_type,
            resource_category,
            action,
        }: PermissionInput,
    ) -> Result<()> {
        let txn = ctx.transaction();

        // Resolve category reference to ID
        let resource_category_id = match resource_category {
            Some(reference) => {
                resolve_category_reference(ctx, site_id, resource_type, reference).await?
            }
            None => None,
        };

        let make_error = || {
            Error::new(
                format!(
                    "failed to add permission {}:{}:{} to role ID {}",
                    resource_type,
                    resource_category_id.unwrap_or(0),
                    action,
                    role_id
                ),
                ErrorType::AddRolePermission,
            )
        };

        role_permission::ActiveModel {
            role_id: Set(role_id),
            site_id: Set(site_id),
            resource_type: Set(resource_type.to_string()),
            resource_category_id: Set(resource_category_id),
            action: Set(action.to_string()),
            ..Default::default()
        }
        .insert(txn)
        .await
        .or_raise(make_error)?;

        Ok(())
    }

    pub async fn remove_permission_from_role(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        role_id: i64,
        PermissionInput {
            resource_type,
            resource_category,
            action,
        }: PermissionInput,
    ) -> Result<()> {
        let txn = ctx.transaction();

        // Resolve category reference to ID
        let resource_category_id = match resource_category {
            Some(reference) => {
                resolve_category_reference(ctx, site_id, resource_type, reference).await?
            }
            None => None,
        };

        let make_error = || {
            Error::new(
                format!(
                    "failed to remove permission {}:{}:{} from role ID {}",
                    resource_type,
                    resource_category_id.unwrap_or(0),
                    action,
                    role_id
                ),
                ErrorType::RemoveRolePermission,
            )
        };

        role_permission::ActiveModel {
            role_id: Set(role_id),
            site_id: Set(site_id),
            resource_type: Set(resource_type.to_string()),
            resource_category_id: Set(resource_category_id),
            action: Set(action.to_string()),
            ..Default::default()
        }
        .delete(txn)
        .await
        .or_raise(make_error)?;

        Ok(())
    }

    pub async fn get_permissions_for_role(
        ctx: &ServiceContext<'_>,
        role_id: i64,
    ) -> Result<Vec<RolePermissionModel>> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!("failed to get permissions for role ID {}", role_id),
                ErrorType::Role,
            )
        };

        let role_permissions = RolePermission::find()
            .filter(role_permission::Column::RoleId.eq(role_id))
            .all(txn)
            .await
            .or_raise(make_error)?;

        Ok(role_permissions)
    }

    // Lambda to query roles that have the specified permission, optionally filtered by category
    async fn query_roles_with_permission(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        resource: Resource,
        resource_category_id: Option<i64>,
        action: Action,
    ) -> Result<Vec<i64>> {
        let txn = ctx.transaction();
        let resource_str = resource.to_string();
        let action_str = action.to_string();

        let category_condition = match resource_category_id {
            Some(id) => role_permission::Column::ResourceCategoryId.eq(id),
            None => role_permission::Column::ResourceCategoryId.is_null(),
        };

        Ok(RolePermission::find()
            .filter(role_permission::Column::SiteId.eq(site_id))
            .filter(role_permission::Column::ResourceType.eq(&resource_str))
            .filter(category_condition)
            .filter(role_permission::Column::Action.eq(&action_str))
            .all(txn)
            .await
            .or_raise(|| Error::new("Error querying permissions", ErrorType::Permission))?
            .into_iter()
            .map(|p| p.role_id)
            .collect::<Vec<_>>())
    }

    pub async fn check_user_can(
        ctx: &ServiceContext<'_>,
        perm_ctx: CheckPermissionContext<'_>,
    ) -> Result<bool> {
        let CheckPermissionContext {
            user_id,
            site_id,
            page_reference,
            permission:
                PermissionInput {
                    resource_type,
                    resource_category,
                    action,
                },
        } = perm_ctx;

        // Resolve category reference to ID for permission checking
        let resource_category_id = match resource_category {
            Some(reference) => {
                resolve_category_reference(ctx, site_id, resource_type, reference).await?
            }
            None => None,
        };

        let make_error = || {
            Error::new(
                format!(
                    "failed to check permission on resource {}:{} with action {}",
                    resource_type,
                    resource_category_id.unwrap_or(0),
                    action
                ),
                ErrorType::Permission,
            )
        };

        // Get all roles for this user (includes virtual roles)
        let role_ids: Vec<i64> = RoleService::get_all_roles_for_user_and_site(
            ctx,
            GetUserRolesInput {
                user_id,
                site_id,
                page_reference,
            },
        )
        .await
        .or_raise(make_error)?
        .into_iter()
        .map(|r| r.role_id)
        .collect();

        if role_ids.is_empty() {
            return Ok(false);
        }

        // Lambda to check if user has any of the specified roles
        let user_has_any_role = |granted_roles: &[i64]| -> bool {
            granted_roles
                .iter()
                .any(|role_id| role_ids.contains(role_id))
        };

        // Check permission based on category specificity
        let roles_with_permission = {
            // Check first for presence of scoped permission
            let specific_roles = match resource_category_id {
                Some(category_id) => {
                    Self::query_roles_with_permission(
                        ctx,
                        site_id,
                        resource_type,
                        Some(category_id),
                        action,
                    )
                    .await
                    .or_raise(make_error)?
                }
                None => vec![],
            };

            if specific_roles.is_empty() {
                // If no scoped permission, check for _default permission
                Self::query_roles_with_permission(
                    ctx,
                    site_id,
                    resource_type,
                    None,
                    action,
                )
                .await
                .or_raise(make_error)?
            } else {
                specific_roles
            }
        };

        Ok(user_has_any_role(&roles_with_permission))
    }
}
