pub use sea_orm_migration::prelude::*;

mod m20220801_214614_create_users_table;
mod m20220801_214627_create_transactions_table;
mod m20220801_214641_create_tags_table;
mod m20220801_214650_create_transactiontags_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220801_214614_create_users_table::Migration),
            Box::new(m20220801_214627_create_transactions_table::Migration),
            Box::new(m20220801_214641_create_tags_table::Migration),
            Box::new(m20220801_214650_create_transactiontags_table::Migration),
        ]
    }
}
