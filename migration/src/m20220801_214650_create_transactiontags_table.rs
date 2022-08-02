use sea_orm_migration::prelude::*;

use crate::m20220801_214627_create_transactions_table::Transactions;
use crate::m20220801_214641_create_tags_table::Tags;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TransactionTags::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TransactionTags::TagId).integer().not_null())
                    .col(
                        ColumnDef::new(TransactionTags::TransactionId)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("tag_id")
                            .from(TransactionTags::Table, TransactionTags::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("transaction_id")
                            .from(TransactionTags::Table, TransactionTags::TransactionId)
                            .to(Transactions::Table, Transactions::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .primary_key(
                        Index::create()
                            .col(TransactionTags::TagId)
                            .col(TransactionTags::TransactionId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TransactionTags::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum TransactionTags {
    Table,
    TagId,
    TransactionId,
}
