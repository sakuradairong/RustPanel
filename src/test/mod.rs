/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-07 17:41:48
 * @LastEditTime: 2024-10-26 04:57:18
 * @FilePath: \RustPanel\src\test\mod.rs
 */

use crate::service::db::DBPool;
use diesel::{QueryDsl, RunQueryDsl};
use crate::models::structure::schema::rp_users;

pub async fn demo(pool: &DBPool)-> Result<(), Box<dyn std::error::Error>> {
    println!("Running DB verification...");
    let mut conn = pool.get().map_err(|e| format!("Failed to get DB connection: {}", e))?;
    let count: i64 = rp_users::table.count().get_result(&mut *conn).map_err(|e| format!("Failed to count users: {}", e))?;
    println!("Total users in database: {}", count);
    println!("DB verification passed.");
    Ok(())
}