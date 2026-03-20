/*
 * permission/category.rs
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

use crate::error::Result;
use crate::services::ServiceContext;
use crate::types::{Reference, Resource};

/// Trait for resolving category references (ID or slug) to category IDs.
///
/// Provides a common interface to allow the permission system to work with
/// any resource category type (page, forum, etc.)
///
/// For `Reference::Id`, implementations can return the ID directly without a DB lookup.
/// For `Reference::Slug`, implementations should query the database to resolve the slug.
#[async_trait::async_trait]
pub trait CategoryResolver: Send + Sync {
    /// Resolve a category reference to its numeric ID.
    async fn resolve(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
    ) -> Result<Option<i64>>;
}

pub struct PageCategoryResolver;

#[async_trait::async_trait]
impl CategoryResolver for PageCategoryResolver {
    async fn resolve(
        ctx: &ServiceContext<'_>,
        site_id: i64,
        reference: Reference<'_>,
    ) -> Result<Option<i64>> {
        use crate::services::CategoryService;

        match reference {
            Reference::Id(id) => Ok(Some(id)),
            Reference::Slug(slug) => {
                let category =
                    CategoryService::get_optional(ctx, site_id, Reference::Slug(slug))
                        .await?;
                Ok(category.map(|c| c.category_id))
            }
        }
    }
}

/// Helper function to resolve a category reference to an ID based on resource type.
pub async fn resolve_category_reference(
    ctx: &ServiceContext<'_>,
    site_id: i64,
    resource_type: Resource,
    reference: Reference<'_>,
) -> Result<Option<i64>> {
    match resource_type {
        Resource::Page => PageCategoryResolver::resolve(ctx, site_id, reference).await,
        // TODO: Add other resource types and their resolvers here
        _ => Ok(None),
    }
}
