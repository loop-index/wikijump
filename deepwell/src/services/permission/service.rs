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
use crate::error::{Error, ErrorType};
use crate::models::permission::{self, Entity as Permission, Model as PermissionModel};
use crate::models::prelude::UserRole;
use crate::models::prelude::{Role, RolePermission};
use crate::models::role::Model as RoleModel;
use crate::models::role_permission;
use crate::models::role_permission::Model as RolePermissionModel;
use crate::models::{role, user_role};
use crate::services::ServiceContext;
use crate::services::audit::{AuditEvent, AuditService};
use crate::services::role::{GetUserRolesInput, RoleService};
use crate::types::{Action, PermissionReference, Resource};

#[derive(Debug)]
pub struct PermissionService;

#[allow(dead_code)] // TEMP
impl PermissionService {
    pub async fn create(
        ctx: &ServiceContext<'_>,
        PermissionInput {
            description,
            resource_type,
            action,
        }: PermissionInput,
    ) -> Result<PermissionOutput> {
        let txn = ctx.transaction();

        // Insert permission
        let model = permission::ActiveModel {
            description: Set(description.clone()),
            resource_type: Set(resource_type.clone()),
            action: Set(action.clone()),
            ..Default::default()
        };

        let make_error = || {
            Error::new(
                format!(
                    "failed to create permission for action {} on resource type {}",
                    action, resource_type
                ),
                ErrorType::Permission,
            )
        };

        let PermissionModel { permission_id, .. } =
            model.insert(txn).await.or_raise(make_error)?;

        Ok(PermissionOutput {
            permission_id,
            description,
            resource_type,
            action,
        })
    }

    pub async fn add_permission_to_role(
        ctx: &ServiceContext<'_>,
        role_id: i64,
        permission_id: i64,
    ) -> Result<()> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!(
                    "failed to add permission ID {} to role ID {}",
                    permission_id, role_id
                ),
                ErrorType::AddRolePermission,
            )
        };

        role_permission::ActiveModel {
            role_id: Set(role_id),
            permission_id: Set(permission_id),
        }
        .insert(txn)
        .await
        .or_raise(make_error)?;

        Ok(())
    }

    pub async fn remove_permission_from_role(
        ctx: &ServiceContext<'_>,
        role_id: i64,
        permission_id: i64,
    ) -> Result<()> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!(
                    "failed to remove permission ID {} from role ID {}",
                    permission_id, role_id
                ),
                ErrorType::RemoveRolePermission,
            )
        };

        role_permission::ActiveModel {
            role_id: Set(role_id),
            permission_id: Set(permission_id),
        }
        .delete(txn)
        .await
        .or_raise(make_error)?;

        Ok(())
    }

    pub async fn get_permission_ids_for_role(
        ctx: &ServiceContext<'_>,
        role_id: i64,
    ) -> Result<Vec<i64>> {
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

        let permission_ids = role_permissions
            .iter()
            .map(|perm| perm.permission_id)
            .collect();

        Ok(permission_ids)
    }

    pub async fn check_user_has_permission(
        ctx: &ServiceContext<'_>,
        user_id: i64,
        site_id: i64,
        permission_id: i64,
    ) -> Result<bool> {
        let txn = ctx.transaction();

        let make_error = || {
            Error::new(
                format!(
                    "failed to check user ID {} for permission ID {}",
                    user_id, permission_id
                ),
                ErrorType::Permission,
            )
        };

        // Get all the roles the user has for this site
        let role_ids: Vec<i64> = RoleService::get_all_roles_for_user_and_site(
            ctx,
            GetUserRolesInput {
                user_id: Some(user_id),
                site_id,
                page_reference: None,
            },
        )
        .await
        .or_raise(make_error)?
        .into_iter()
        .map(|ur| ur.role_id)
        .collect();

        // Check if any of those roles have the permission
        let exists = RolePermission::find()
            .filter(role_permission::Column::RoleId.is_in(role_ids))
            .filter(role_permission::Column::PermissionId.eq(permission_id))
            .one(txn)
            .await
            .or_raise(make_error)?
            .is_some();

        Ok(exists)
    }

    pub async fn check_user_can(
        ctx: &ServiceContext<'_>,
        user_id: i64,
        site_id: i64,
        resource_type: Resource,
        action: Action,
    ) -> Result<bool> {
        let make_error = || {
            Error::new(
                format!(
                    "failed to check if user ID {} can {} {}",
                    user_id, action, resource_type
                ),
                ErrorType::Permission,
            )
        };

        let permission: PermissionModel = Self::get(
            ctx,
            PermissionReference::ResourceAction(resource_type, action),
        )
        .await?;

        let has_permission = Self::check_user_has_permission(
            ctx,
            user_id,
            site_id,
            permission.permission_id,
        )
        .await
        .or_raise(make_error)?;

        Ok(has_permission)
    }

    pub async fn get_optional(
        ctx: &ServiceContext<'_>,
        reference: PermissionReference,
    ) -> Result<Option<PermissionModel>> {
        let txn = ctx.transaction();

        let make_error =
            || Error::new("failed to fetch permission", ErrorType::Permission);

        let condition = match reference {
            PermissionReference::Id(id) => permission::Column::PermissionId.eq(id),
            PermissionReference::ResourceAction(resource, action) => {
                permission::Column::ResourceType
                    .eq(resource.to_string())
                    .and(permission::Column::Action.eq(action.to_string()))
            }
        };

        let permission = Permission::find()
            .filter(condition)
            .one(txn)
            .await
            .or_raise(make_error)?;

        Ok(permission)
    }

    #[inline]
    pub async fn get(
        ctx: &ServiceContext<'_>,
        reference: PermissionReference,
    ) -> Result<PermissionModel> {
        find_or_error!(Self::get_optional(ctx, reference), "permission", Permission)
    }
}
