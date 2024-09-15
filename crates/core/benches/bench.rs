use alloy::primitives::{address, Address};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eql_core::interpreter::Interpreter;
use pprof::criterion::{Output, PProfProfiler};
use std::error::Error;
use tokio::runtime::Runtime;

// This list likely contains duplicate addresses
static ACCOUNTS: [Address; 100] = [
    address!("c71048d303920c73c29705192393c567ac4e6c67"),
    address!("b0fb2791c05416c41eba718502a738a0e2cba77e"),
    address!("5e47cf975c6abd973a7f135b7b86881308a72cc9"),
    address!("64be26372f68049c86bacc297a3c8fe1cb8b458e"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("3fdb8e26a9f53b78216c63bbc7a5c849701d183e"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("a8a4bdd2ea50e07e17582088da02bbc3720a0489"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("5c34e725cca657f02c1d81fb16142f6f0067689b"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("aa8167de65ac8951492fc20b43929a2ba085c8a7"),
    address!("0e42acbd23faee03249daff896b78d7e79fbd58e"),
    address!("d5e66c5a5b991a312ff8f970216417411aa607cf"),
    address!("d761dd69c7d9e4173938604b3c42910e8689e6f3"),
    address!("9accd33c5a81ffe084036f6b68d8f4cdd4a7cafe"),
    address!("c36442b4a4522e871399cd717abdd847ab11fe88"),
    address!("95222290dd7278aa3ddd389cc1e1d165cc4bafe5"),
    address!("4befa2aa9c305238aa3e0b5d17eb20c045269e9d"),
    address!("ae2fc483527b8ef99eb5d9b44875f005ba1fae13"),
    address!("6b75d8af000000e20b7a7ddf000ba900b4009a80"),
    address!("ea3ea09394f5e925ae7592dff1d675558d0831fd"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("7c925c1286654d3b720a1293d8d0821d12ebe335"),
    address!("f3de3c0d654fda23dad170f0f320a92172509127"),
    address!("889823b1163174a05a42128b69b4e80243b88d0e"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("94dd5e55f9b4fcf9968f2102ed46ed2762f88211"),
    address!("e43ca1dee3f0fc1e2df73a0745674545f11a59f5"),
    address!("ae2fc483527b8ef99eb5d9b44875f005ba1fae13"),
    address!("6b75d8af000000e20b7a7ddf000ba900b4009a80"),
    address!("99b2c5d50086b02f83e791633c5660fbb8344653"),
    address!("f4f748d45e03a70a9473394b28c3c7b5572dfa82"),
    address!("2aa08118a4967531fe759dcc7fde1a655037f293"),
    address!("5ddf30555ee9545c8982626b7e3b6f70e5c2635f"),
    address!("54620b9a8a2c43aa8ed028450a7ce656a9c69feb"),
    address!("f114144993f279429ee8953837f0332c7ddbfb39"),
    address!("ceaad947f68f5097d36e613ea182284f56725c1b"),
    address!("80a64c6d7f12c47b7c66c5b4e20e72bc1fcd5d9e"),
    address!("11235534a66a33c366b84933d5202c841539d1c9"),
    address!("22ce84a7f86662b78e49c6ec9e51d60fdde7b70a"),
    address!("d267fda7ecbe322e64bf807de06e8e2284b72da9"),
    address!("d42b0ecf8a9f8ba9db7b0c989d73cf0bd5f83b66"),
    address!("26fd09c8b44af53df38a9bad41d5abc55a1786af"),
    address!("cb83ca9633ad057bd88a48a5b6e8108d97ad4472"),
    address!("5eb4dd17f59bcbc86f98cd459b01b8fe650b321b"),
    address!("111111125421ca6dc452d289314280a0f8842a65"),
    address!("cf46235341b6329ed5a72cca00ca286a02d5c7cb"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("0e847b5cf7fe5ae3122f30c303e42aa4bcab713b"),
    address!("0000000000000068f116a894984e2db1123eb395"),
    address!("e9a22994c2fd147d9d753709b4a208876f3fb20e"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("771e47c039ec97460f97d7ed0f033df70ebdcb78"),
    address!("ec53bf9167f50cdeb3ae105f56099aaab9061f83"),
    address!("079c298311e7d376a845c8d4756998b256a947b2"),
    address!("b131f4a55907b10d1f0a50d8ab8fa09ec342cd74"),
    address!("a5b1edac64871b302dc94b0eca144738f9f43d80"),
    address!("81e019dc564d5f906e0915e3818f69cc4b3c0804"),
    address!("7077eb3743436724cc16c8689df4659366b6876f"),
    address!("7a250d5630b4cf539739df2c5dacb4c659f2488d"),
    address!("000000633b68f5d8d3a86593ebb815b4663bcbe0"),
    address!("06a9ab27c7e2255df1815e6cc0168d7755feb19a"),
    address!("148e738c265849c257df1dbcd0faf8545b2f7575"),
    address!("000000000000000000000000000000000000dead"),
    address!("16a8cbcec6995c5f78ef4de7fd64a373723337b3"),
    address!("dac17f958d2ee523a2206206994597c13d831ec7"),
    address!("0e9e39190df7a2d38872736910cddfa519e419ca"),
    address!("6c24739662e621616f5f7cdac25d61f9fd29eee0"),
    address!("2af4d59623ba2eb1e7849b770f945b6791ef0f3c"),
    address!("9ff62234b9081268b9ac1ed412862e79cfbd69f7"),
    address!("c73d58d6bd8ab54c48ab20843cb3d749e6177569"),
    address!("cbd6832ebc203e49e2b771897067fce3c58575ac"),
    address!("45fac002da1c0b7cd2c972f936b1fba58cdca678"),
    address!("cbd6832ebc203e49e2b771897067fce3c58575ac"),
    address!("cb1ada11b21fe066dcb91a12cb8195fafa50420b"),
    address!("b0999731f7c2581844658a9d2ced1be0077b7397"),
    address!("338b9920971dcaf5b6d83b6b292231110d3bec40"),
    address!("79c06846633123d23ed6a71c456bb2ff09129938"),
    address!("478381e7cde64b743267051600aa4dcbb3c7d349"),
    address!("7f792db54b0e580cdc755178443f0430cf799aca"),
    address!("cef78848d01d522ab7b53e3d7382d36f6c3c2f23"),
    address!("6ebe066da43605ed52d6e7eaaf6bd0dc64758238"),
    address!("e0fa8dc4f284f12e0361f8ad3e5d26a060b60e60"),
    address!("6463fdae77d09d92d72b8a5418c54117abbc0f21"),
    address!("4e14df2b1fa2f938171363a7461ea00389f2f337"),
    address!("dac17f958d2ee523a2206206994597c13d831ec7"),
    address!("b2882dbba1d8c1549bdde1dad3d26c1e307c3c73"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("0244f7204b9c554306053cc557e14d6cbd40a33c"),
    address!("ffff000000000000000000000000000000012553"),
    address!("579f82db0dcb0c8d775268fc364f7a8bce6b024d"),
    address!("c6a37968f10187d0bf1af15875d01f89943ad5dc"),
    address!("0c1296145b7f0899647ddb640592f53345d1d7bd"),
    address!("3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad"),
    address!("3415142c25e9407351419dc28eaa00dd6d175157"),
    address!("a9d1e08c7793af67e9d92fe308d5697fb81d3e43"),
    address!("1e1247dcef65656f29f670656dc37a92354fd640"),
    address!("3cd751e6b0078be393132286c442345e5dc49699"),
    address!("a64961b4be05f2d81345c8b414b47950d2a7accb"),
];

async fn fetch_accounts_interpreter(num_of_accounts: usize) -> Result<(), Box<dyn Error>> {
    if num_of_accounts == 0 || num_of_accounts > 100 {
        return Err("Number of accounts must be between 1 and 100".into());
    }

    println!("Fetching {} accounts", num_of_accounts);

    let joined_accounts = ACCOUNTS[1..num_of_accounts]
        .iter()
        .map(|account| format!("\"{}\"", account))
        .collect::<Vec<String>>()
        .join(", ")
        .replace("\"", "");

    Interpreter::run_program(&format!("GET * FROM account {joined_accounts} ON eth")).await?;
    Ok(())
}

async fn fetch_blocks_interpreter(end_block: u64) -> Result<(), Box<dyn Error>> {
    println!("Fetching {} blocks", end_block);
    Interpreter::run_program(&format!("GET * FROM block 1:{end_block} ON eth")).await?;
    Ok(())
}

async fn dump_blocks_query_builder(end_block: u64, format: &str) -> Result<(), Box<dyn Error>> {
    println!("Dumping {} blocks in {} format", end_block, format);
    Interpreter::run_program(&format!(
        "GET * FROM block 1:{end_block} ON eth > file.{format}"
    ))
    .await?;
    Ok(())
}

fn bench_dump_10_block_csv(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Dump CSV - 10 block rows", |b| {
        b.to_async(&rt)
            .iter(|| dump_blocks_query_builder(black_box(10), black_box("csv")))
    });
}

fn bench_dump_10_block_json(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Dump JSON - 10 block rows", |b| {
        b.to_async(&rt)
            .iter(|| dump_blocks_query_builder(black_box(10), black_box("json")))
    });
}

criterion_group! {
    name = dump_10_rows;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench_dump_10_block_csv, bench_dump_10_block_json
}

fn bench_fetch_100_blocks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Fetch 100 blocks", |b| {
        b.to_async(&rt)
            .iter(|| fetch_blocks_interpreter(black_box(100)))
    });
}

fn bench_fetch_100_accounts(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Dump 100 accounts", |b| {
        b.to_async(&rt)
            .iter(|| fetch_accounts_interpreter(black_box(100)))
    });
}

criterion_group! {
    name = fetch_100_rows;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench_fetch_100_blocks, bench_fetch_100_accounts
}

criterion_main!(dump_10_rows, fetch_100_rows);
