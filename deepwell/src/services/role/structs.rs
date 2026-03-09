/*
 * services/role/structs.rs
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
use crate::types::{Maybe, Reference};
use time::OffsetDateTime;

#[derive(Deserialize, Debug, Clone)]
pub struct CreateRoleInput {
    pub site_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_virtual: bool,
    pub is_system: bool,
    pub level: i32,
}

#[derive(Serialize, Debug, Clone)]
pub struct CreateRoleOutput {
    pub role_id: i64,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct UpdateRoleInput {
    pub name: Maybe<String>,
    pub description: Maybe<String>,
    pub level: Maybe<i32>,
    pub permission_ids: Maybe<Vec<i64>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GrantUserRoleInput {
    pub user_id: i64,
    pub role_id: i64,
    pub assigning_user_id: i64,
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetUserRolesInput {
    pub site_id: i64,
    pub user_id: Option<i64>,
    pub page_reference: Option<Reference<'static>>,
}
