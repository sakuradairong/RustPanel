/*
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-09 17:20:04
 * @LastEditTime: 2024-05-13 21:29:55
 * @FilePath: \RustPanel\src\models\structure\schema.rs
 */
use diesel::prelude::*;

table! {
    rp_users (id) {
        id -> Integer,
        username -> Varchar,
        password -> Varchar,
        salt -> Varchar,
        authority -> Varchar,
        error_count -> Integer,
        status -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        password_algo -> Varchar,
        must_change_password -> Integer,
        token_version -> Integer,
    }
}

table! {
    invitations (id) {
        id -> Integer,
        username -> Varchar,
        password -> Varchar,
        authority -> Varchar,
        expires_at -> Timestamp,
    }
}
allow_tables_to_appear_in_same_query!(rp_users,invitations);
