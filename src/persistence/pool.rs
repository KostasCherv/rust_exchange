//! Database pool and migrations.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Create a pool from `DATABASE_URL` and run migrations.
pub async fn create_pool_and_migrate(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    run_migrations(&pool).await?;
    Ok(pool)
}

/// Run embedded migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
