#![no_main]
use libfuzzer_sys::fuzz_target;
use psl_agent_contracts::{SwapContract, TernaryProgram};

fuzz_target!(|data: &[u8]| {
    let c = SwapContract::build();
    let _ = c.run(data);
});
