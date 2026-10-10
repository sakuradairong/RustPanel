/*
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-08 21:47:26
 * @LastEditTime: 2024-05-13 21:30:09
 * @FilePath: \RustPanel\src\models\structure\rp_users.rs
 */

use diesel::prelude::*;
use serde::{Deserialize, Serialize};


#[derive(Queryable)]
pub struct Users {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub salt: String,
    pub authority: String,
    pub error_count: i32,
    pub status: i32,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub password_algo: String,
    pub must_change_password: i32,
    pub token_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlimUser {
    pub id: i32,
    pub username: String,
    pub authority: String,
    pub created_at: chrono::NaiveDateTime,
}

impl From<Users> for SlimUser {
    fn from(user: Users) -> Self {
        SlimUser {
            id: user.id,
            username: user.username,
            authority: user.authority,
            created_at: user.created_at,
        }
    }
}
