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
use std::error::Error;
use tokio::runtime::Runtime;

async fn dump_blocks(end_block: u64, format: DumpFormat) -> Result<(), Box<dyn Error>> {
    println!("Dumping {} blocks in {} format", end_block, format);

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
            vec![EntityId::Block(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(end_block)),
            ))],
        )
        .on(Chain::Ethereum)
        .dump(Dump::new("bech".to_string(), format))
        .run()
        .await?;

    Ok(())
}

fn bench_dump_10_block_csv(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Dump CSV - 10 block rows", |b| {
        b.to_async(&rt)
            .iter(|| dump_blocks(black_box(10), black_box(DumpFormat::Csv)))
    });
}

fn bench_dump_10_block_json(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Dump JSON - 10 block rows", |b| {
        b.to_async(&rt)
            .iter(|| dump_blocks(black_box(10), black_box(DumpFormat::Json)))
    });
}

criterion_group!(
    dump_benches,
    bench_dump_10_block_csv,
    bench_dump_10_block_json
);
criterion_main!(dump_benches);
