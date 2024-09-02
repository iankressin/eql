use std::error::Error;

use alloy::eips::BlockNumberOrTag;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eql_core::common::{
    chain::Chain,
    entity::Entity,
    entity_filter::BlockRange,
    entity_id::EntityId,
    query_builder::EQLBuilder,
    types::{BlockField, Dump, DumpFormat, Field},
};

async fn dump_blocks_to_csv(end_block: u64) -> Result<(), Box<dyn Error>> {
    EQLBuilder::new()
        .get(vec![
            Field::Block(BlockField::Hash),
            Field::Block(BlockField::Number),
            Field::Block(BlockField::Size),
            Field::Block(BlockField::Timestamp),
            Field::Block(BlockField::StateRoot),
        ])
        .from(
            Entity::Block,
            EntityId::Block(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(end_block)),
            )),
        )
        .on(Chain::Ethereum)
        .dump(Dump::new("bech".to_string(), DumpFormat::Csv))
        .run()
        .await?;

    Ok(())
}

fn bench_dump_10_block_csv(c: &mut Criterion) {
    c.bench_function("Dump CSV - 10 block rows", |b| {
        b.iter(|| dump_blocks_to_csv(black_box(10)))
    });
}

criterion_group!(dump_benches, bench_dump_10_block_csv);
criterion_main!(dump_benches);
