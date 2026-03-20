use anyhow::Result;
use sqlx::PgPool;

const INIT_SQL: &str = include_str!("../../migrations/001_initial.sql");

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    // Check if tables already exist
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'accounts')",
    )
    .fetch_one(pool)
    .await?;

    if !exists {
        tracing::info!("Creating database schema...");
        sqlx::raw_sql(INIT_SQL).execute(pool).await?;
        tracing::info!("Database schema created successfully");
    } else {
        tracing::info!("Database schema already exists, skipping migration");
    }

    Ok(())
}
