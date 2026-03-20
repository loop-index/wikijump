/*
 * services/permission/permissions
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
use crate::types::Reference;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::str::FromStr;
use strum_macros::{Display, EnumString};

#[derive(
    Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, Display, Serialize,
)]
#[strum(serialize_all = "kebab_case", ascii_case_insensitive)]
pub enum Resource {
    Page,
    Role,
}

#[derive(
    Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, Display, Serialize,
)]
#[strum(serialize_all = "kebab_case", ascii_case_insensitive)]
pub enum Action {
    View,
    Edit,
    Create,
    Delete,
    Rename,
    Assign,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct Permission {
    pub resource: Resource,
    pub resource_category: Option<Reference<'static>>,
    pub action: Action,
}

impl FromStr for Permission {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split(':').collect::<Vec<_>>();
        let (resource, resource_category, action) = match parts.as_slice() {
            [resource, action] => (resource, None, action),
            [resource, resource_category, action] => {
                // Try to parse as ID first, fall back to treating as slug
                let reference = resource_category
                    .parse::<i64>()
                    .map(Reference::Id)
                    .unwrap_or_else(|_| {
                        Reference::Slug(Cow::Owned(resource_category.to_string()))
                    });
                (resource, Some(reference), action)
            }
            _ => return Err("invalid permission format"),
        };

        Ok(Self {
            resource: Resource::from_str(resource)
                .map_err(|_| "invalid resource type")?,
            resource_category,
            action: Action::from_str(action).map_err(|_| "invalid action type")?,
        })
    }
}
