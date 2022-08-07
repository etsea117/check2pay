use sea_orm_migration::prelude::*;

use crate::m20220801_214614_create_users_table::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Transactions::Date).date().not_null())
                    .col(
                        ColumnDef::new(Transactions::Amount)
                            .decimal_len(14, 4)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Transactions::Expense).boolean().not_null())
                    .col(ColumnDef::new(Transactions::Note).string().null())
                    .col(ColumnDef::new(Transactions::UserId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("user_id")
                            .from(Transactions::Table, Transactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub enum Transactions {
    Table,
    Id,
    Date,
    Amount,
    Expense,
    Note,
    UserId,
}
