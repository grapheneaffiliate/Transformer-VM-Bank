#![no_main]
use libfuzzer_sys::fuzz_target;
use psl_agent_contracts::{TernaryProgram, TransferContract};

fuzz_target!(|data: &[u8]| {
    let c = TransferContract::build();
    let _ = c.run(data);
});
