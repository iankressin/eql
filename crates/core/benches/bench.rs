use alloy::primitives::{address, b256, Address, B256};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eql_core::interpreter::Interpreter;
use pprof::criterion::{Output, PProfProfiler};
use std::error::Error;
use tokio::runtime::Runtime;

static TRANSACTIONS: [B256; 100] = [
    b256!("fffc07e7ff65a5ff7f8496042f85fc4e1d6bd29e012e776b970f4414c07d4d41"),
    b256!("fff84103f53c5182b50e98912a5aa78ec7c3f14677727e2092db242d020084b3"),
    b256!("fff1833dd47d350510e066f40c8683af8aa6b7879c9c967a74d7b913a7119d05"),
    b256!("ffefde26bf3a54d65c50ed248d649dba29827353d81a177cd2f92493250c5ca7"),
    b256!("ffed9efba6ed9a1a5ae695be314737cd9fae729398a4661390a9d68fd4d16e4c"),
    b256!("ffeb39d725ab32552f740a6cb4002436a83221997879bd89e972c9ea1ca5ee3c"),
    b256!("ffeacf8cfdf9a57e84fcf1faaadc4b14a41e93609d3fa5b483810b267578e863"),
    b256!("ffe03e71de0d232503599c0b212cc0fa049991fb4edf6440666249e0853d6529"),
    b256!("ffdaee3428257f1947d51610d65700e1b88e320c0a0ce001c7cfaad06da29bb2"),
    b256!("ffd73eb33eda6d39f9ad6c6389009d200c4833ae736b9827f117f24bea9355d4"),
    b256!("ffd0a2317b433b34656312d1b1a6979b9f02e2dfa864ba8a754c60ca4b38dfb7"),
    b256!("ffd078b9caaaa045c6e6cd59bf919d3ed140701a4b6c07eb480d96eee869ac1d"),
    b256!("ffc574efaaab4d5b0061f6a04ba23e957d5b3f559de73cc68e9c3c8b229f2d20"),
    b256!("ffbed21e837a1fae50b723138731831e813e74ccdd2fe3e404e7a1dc898085e3"),
    b256!("ffbc9941ab591f0795a6addccdf9d9bb78318d2379f76e83326a67799003ffa8"),
    b256!("ffbc2bf09ea1fca547f3e15d96f99a28d42a85b689d2c6be123ec3920bf9d584"),
    b256!("ffba4d5bbaa4009b3f8496c56f1a62fbc040447fbfc51881995c97e351db1513"),
    b256!("ffb62c844b7bad4d7000a8a85146980cf06baa10ef6e5c094a2b3c9bdeeb509a"),
    b256!("ffb621c7bb813eab009c09977b2e5c7a7cef590b60ae80a9771efb2389676aa7"),
    b256!("ffb3e4282601ee675cea99739531cb7d332458f99187d4dc077c8cbf152e4929"),
    b256!("ffb12312b042ee30c692b59129bf83a6ba9709c6ba5963a07ca11288a75c7f50"),
    b256!("ffa8b376828648230bcf99a394a4975dac5ad798da4654fe99db47914a88654a"),
    b256!("ff9e3ce8bbf557eb4ea8fd692f008830bdca8f2a69d4f6ddefe1631bcd7481dc"),
    b256!("ff976c04def9e0e9318653721306911a404da81b2a55215452db1aa702bc46bd"),
    b256!("ff976295847aa86710f68aaafee7b4b4a81cd63e5adf577cfa80f757a30ef49e"),
    b256!("ff944f100b43733364930cd997c955cc23bbdb1a8804759c5f42e44969980ba9"),
    b256!("ff90f505a4e65ebe353859a6fb466f1c35cf359a4d5ed0b0d8e0732b36840e2d"),
    b256!("ff90c56a236ff338bb8fd41b1773c041835f4da6ce925c79474ed11a1320f165"),
    b256!("ff8e8dae4ab970d9681fe2f953b2f95f590f219ecb27cb0635613df73bc87963"),
    b256!("ff8dfe5301dc42c846a41724da427634f90ff562367fec897a0defc1164028f7"),
    b256!("ff8a6f0a7fe42239330d7ff24457bc41d473513b40c1ddade81ef0ccdd416b8d"),
    b256!("ff7281b198336b5af458df396e672dd11a3e13cad695fb83150bd0baff47bcb2"),
    b256!("ff713e5902ac991f32476775bfadfd17eb4f013720f6eaeeb1e16ece666f0404"),
    b256!("ff6e2e2adfaf7e605f8afca143ebde4fb21695c00b8e4663a1b46296e8e363ab"),
    b256!("ff6defbe324636fd68ee8fca3430e6586f8ee0341a6a89ef120d0a2b804e6594"),
    b256!("ff6c9c31e207ce91d9e8e87ed4202b86ebd30062763a37a58315b193f50a664a"),
    b256!("ff6c8aa55724968d4cc00acc172b53ed9d6d3a53a23d003332307bffa7d149d1"),
    b256!("ff6aff82389df346783cdd21e8f4e1b5d7b5cdd323737ea129d12ff2354b2feb"),
    b256!("ff5a55ece8cbed07144de3ef02e857aa207ff80333f239e0ff6d754af4765c9e"),
    b256!("ff59e256d063b2ff6625a6ca5108592ef4d060b3485c27f462295975adf35383"),
    b256!("ff55cd79b30c7c5a0e6976932503f5569c3a20da919a8b4f7f431347d90d2736"),
    b256!("ff5217eafbac29e46883871db859b57804a0cd936bf2cc7ee0e2e30067df262f"),
    b256!("ff4c8aebc1004f9fa8ebbee7bcf88bf5c7ae017f643b38d35fb290114e5e6d08"),
    b256!("ff4be286d948f6a986108c20cf734cd1b841d73db75ee2258bc03eb1779b9a84"),
    b256!("ff4b616492eeef4ec31131acd412ed37857104cfcc274e452a09efbd78ba6300"),
    b256!("ff437f851f31b5a2bea66816c33a613b610270513024f47a716364b2fe6187f2"),
    b256!("ff430a23e92111db73ac83a187b4d6141a9619173d88db2f5d8f10256880a8b6"),
    b256!("ff40812a3be57738edd462c8be1568082430214c33223c2a6f6c27122d358370"),
    b256!("ff3fdb6faaa527df03e0826d4b6ed12b7e475ab6d5e703b6f0b6f8a6c3afa69a"),
    b256!("ff3d417852d138232dfd1edf5e7938471f243b2aa3f1c2b3430506dde5850bfc"),
    b256!("ff1f82e656f21cfa2f4f41755bb4407beba9a37c3985462400ebeeaf2eebc26e"),
    b256!("ff1cfc8fe289bf9f7b7e4bb4421b85f2d0cff0fb292cab14426a854039a3c486"),
    b256!("ff17a4c2d6117550c50aa80e2ff96002850a8bec413599891ab28d6e6f836ac6"),
    b256!("ff158d10606372b1eab2d11f3dd5cdece99e3188200441e6fe863730bdc92267"),
    b256!("ff15311c0d1ed6538ec580620359574f110c73b0c21157abb0922dc850e28a20"),
    b256!("ff12cc82ead348b7967f507615235d8988d778743b35f17f07cb286eea6c5477"),
    b256!("ff12966f55eb300f6637b0c170addf9f451f59b42ecf208e7bcf8c23f8558923"),
    b256!("ff0e9ba1ac930c93cfb3f41dd89e795ddc9dacc38a973055665193fcef5e6090"),
    b256!("ff0db0ce017ef1ce46810d535904936d9ab2acd8febe70c582a9b5e994cbf0be"),
    b256!("ff0beaa49272138db9a1a7f90c5f6caa5fc480febac827132e18111fe98d247a"),
    b256!("ff0af668082ff6cc7173e6917737a02052c89e23e7e9583a27b6308e12eaa92b"),
    b256!("ff01ec6b29390d1438fdf49f8f373d2e39bfc213add15f09b8163e4de186f1dd"),
    b256!("ff018c40aa32162a7152ef40ac7a9afd46d8bb32dfe21ecc083bab2bffc89083"),
    b256!("ff008f6bee9f1c9d38981991e79fa4999d35dc2bd9280f4ea1b3133d56194398"),
    b256!("fefe9d9e2778fec0baa425b77cc5d75f3009e07c1780143fffe18916cefac148"),
    b256!("fef54a2b664222f01bc73f50c17bb2ca1088b989566dd02dcca553402140b681"),
    b256!("feefba55f15ac60578d826f2ff0c3fd35d1a173e37cf2376204b31227a3720f1"),
    b256!("feedc2cfd0d6f7ea76ae2a2ca27e0548f8f1593528ee701a72ee505964430e1f"),
    b256!("fee40387d39b6853ae365ab599625d961c08cf38d201d5a68a64070d5f2e290a"),
    b256!("fedca4c61221ee8d4e57ce38d8e81e1664c5104451b23e1dac9eb9f349e97911"),
    b256!("fedb94e4e0ddf8be3b84eb5e30cbb4d2c36c2b15726e9ff47b4c71e3069002b3"),
    b256!("fed8502dbe8aed15df519415455187ce65edb2b4c146de7d30f7783f3ed35165"),
    b256!("fed6cabd98cdc12a7df87fa8ab1a10196a7f5befb0720bae7fc4475a8b94306e"),
    b256!("fecf10ea27df0ae238e9c34e952514c27baf5d1f7ce9ae1f1a1bf343ec4eee30"),
    b256!("fec2de231f08fcdde740410744bf19ccfa62b8a9fe3a1aaa4cdca9a23096c285"),
    b256!("feb9400c62bf64fa2b05666e8a89d3aaa0385af277a5ae8ba52cda4e81ad7bda"),
    b256!("feb741c416f4c66b638ee1f0e6030c79385bb998603699d55e2673d0dc28e853"),
    b256!("feb1513155a5131348c3fcc0fbefa51ae92f012612447fa7d1953a4b7c3120ee"),
    b256!("feaeea5fe71afc0791e57a7d3543b4c3c64f19e4623cf5d90d5b5e66a3185465"),
    b256!("feae77dc62a83c597813d01158d4bcd5249f3626d16d69ef88a97510735c6bc9"),
    b256!("feac74a7ebab5c601863917b8526da5060c36b68ef81193485de23abfd4c6181"),
    b256!("feaaec9e67d58bcd279aec0f746778fcbbf61d57b477e4eeb41bf7338754995b"),
    b256!("feaa49b93750e5dec0606ef2a33bf87debfed64b6226c084447b8003e32e7fcd"),
    b256!("fea646df58edfc7ac6839848b19fafefdb0d1082f0508c2c35922b693d2a9ba9"),
    b256!("fea4f7019134fa0f985b0796034c0a9b427f8067d3af8fe1e70b1dc17ab09549"),
    b256!("fea12f8d283e0317c34b4d10427c5b9b6eb8ec8e4ce59fa983099706130b3894"),
    b256!("fe37f8416310b4e69fc24264ac87da50f68be9d15101470e43d9456af4443064"),
    b256!("fe2e51730438d931d9cdc794dc7d2a777e520b06f7559d6b4adbb49c457c9308"),
    b256!("fe2c29e7c0b0a9efb9adcdce2989257a11768c497013ac3ec62972fa14499cf7"),
    b256!("fe24eb273f565802075dc33b2c39e33cb7a098d47e65ae710f8eb04d7635d9d5"),
    b256!("fe2077d45fd61c2dff2dbc74cea62f82f05cea91c82de347169588e0acc9d8cb"),
    b256!("fe1a9b8b69f18cc3bd140c48ff976201e79faffeea3052d4fcaebc5fac91cc64"),
    b256!("fe1a6677f31cd2f5192c28d0d35b05dd7ef5c15e8a2ee0b7b1e191ad787e334e"),
    b256!("fe1259ab2ea68a4ea749408e731c6baed46128d599e450fbab0eba1f855d6257"),
    b256!("fe0c46529cd6c51fca6e936376525e29236b4285f4b932d48e98d21e55740ade"),
    b256!("fe064362a3076f3ac347aa8d7e57f3a8a3cf5ac92ecbeb18fe3d00e58525aac2"),
    b256!("fe044c720742c0125e18438351e469014c6435a965ca29091200da12222e01f8"),
    b256!("fe024d156b74be1b3fe6d94b2ff5b963c5b941e2446cbdf98a63784c687c88f2"),
    b256!("fdfee1267fdc457cbe613ee1abc9a2673726f3a701a7474a048c355fc182325e"),
    b256!("fdfca2586e535ca82aaec703560fd168fd298aaab69485bb361b5e2de9027525"),
];

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

async fn fetch_transactions_interpreter(num_of_transactions: usize) -> Result<(), Box<dyn Error>> {
    if num_of_transactions == 0 || num_of_transactions > 100 {
        return Err("Number of transactions must be between 1 and 100".into());
    }

    println!("Fetching {} transactions", num_of_transactions);

    let joined_transactions = TRANSACTIONS[1..num_of_transactions]
        .iter()
        .map(|transaction| format!("\"{}\"", transaction))
        .collect::<Vec<String>>()
        .join(", ")
        .replace("\"", "");
    Interpreter::run_program(&format!("GET * FROM tx {joined_transactions} ON eth")).await?;
    Ok(())
}

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

async fn fetch_logs_interpreter() -> Result<(), Box<dyn Error>> {
    println!("Fetching logs");
    Interpreter::run_program(
        "GET * FROM log WHERE block 4638657:4638758, address 0xdAC17F958D2ee523a2206206994597C13D831ec7, topic0 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a ON eth"
    ).await?;
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

    c.bench_function("Fetch 100 accounts", |b| {
        b.to_async(&rt)
            .iter(|| fetch_accounts_interpreter(black_box(100)))
    });
}

fn bench_fetch_100_transactions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Fetch 100 transactions", |b| {
        b.to_async(&rt)
            .iter(|| fetch_transactions_interpreter(black_box(100)))
    });
}

fn bench_fetch_logs(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Fetch log", |b| {
        b.to_async(&rt).iter(|| fetch_logs_interpreter())
    });
}

criterion_group! {
    name = fetch_100_rows;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench_fetch_100_blocks, bench_fetch_100_accounts, bench_fetch_100_transactions, bench_fetch_logs

}

criterion_main!(dump_10_rows, fetch_100_rows);
