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
use crate::models::permission::{self, Model as PermissionModel};
use crate::models::role::Model as RoleModel;
use crate::models::role_permission::Model as RolePermissionModel;
use crate::error::{Error, ErrorType};
use crate::error::ErrorType::Permission;
use crate::models::prelude::RolePermission;
use crate::models::role_permission;
use crate::services::audit::{AuditEvent, AuditService};
use crate::services::ServiceContext;

#[derive(Debug)]
pub struct PermissionService;

impl PermissionService {
    pub async fn create(
        ctx: &ServiceContext<'_>,
        PermissionInput {
            description,
            resource_type,
            action
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
        }.insert(txn).await.or_raise(make_error)?;

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
        }.delete(txn).await.or_raise(make_error)?;

        Ok(())
    }
    
    pub async fn get_permission_ids_for_role(
        ctx: &ServiceContext<'_>,
        role_id: i64,
    ) -> Result<Vec<i64>> {
        let txn = ctx.transaction();
        
        let make_error = || Error::new(
            format!("failed to get permissions for role ID {}", role_id),
            ErrorType::Role
        );
        
        let role_permissions = RolePermission::find()
            .filter(
                role_permission::Column::RoleId.eq(role_id)
            )
            .all(txn).await.or_raise(make_error)?;

        let permission_ids = role_permissions.iter().map(|perm| perm.permission_id).collect();
        
        Ok(permission_ids)
    }

    pub async fn get_optional(
        ctx: &ServiceContext<'_>,
        reference: Reference<'_>,
    ) -> Result<Option<PermissionModel>> {
        let txn = ctx.transaction();

        let make_error = || Error::new("failed to fetch permission", ErrorType::Permission);

        let permission = match reference {
            Reference::Id(id) => {
                permission::Entity::find_by_id(id).one(txn).await.or_raise(make_error)?
            }
            _ => None
        };

        Ok(permission)
    }

    #[inline]
    pub async fn get(
        ctx: &ServiceContext<'_>,
        reference: Reference<'_>,
    ) -> Result<PermissionModel> {
        find_or_error!(Self::get_optional(ctx, reference), "permission", Permission)
    }
    
    pub async fn get_permission_from_resource_and_action(
        ctx: &ServiceContext<'_>,
        resource_type: &str,
        action: &str,
    ) -> Result<Option<PermissionModel>> {
        let txn = ctx.transaction();
        let make_error = || Error::new(
            format!("failed to fetch permission for resource type {} and action {}", resource_type, action),
            ErrorType::Permission
        );
        
        let permission = permission::Entity::find()
            .filter(
                permission::Column::ResourceType.eq(resource_type)
                    .and(permission::Column::Action.eq(action))
            )
            .one(txn).await.or_raise(make_error)?;
        
        Ok(permission)
    }
}