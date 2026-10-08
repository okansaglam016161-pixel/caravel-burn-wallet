//! Shared setup: which burn wallet binary the tests run against.
//!
//! By default the burn wallet is compiled from this crate's source, as usual. With
//! `BURN_WALLET_WASM=<path>` set, the burn wallet under test is THAT binary instead — e.g. the exact
//! bytes published on-chain — registered under its published template address
//! (`BURN_WALLET_TEMPLATE`, default the address published on esmeralda). The test-only templates are
//! always compiled from source.

use tari_template_lib::prelude::TemplateAddress;
use tari_template_test_tooling::{Package, TemplateTest};

/// Published on esmeralda on 2026-10-08 (see the README's Live deployment section).
const PUBLISHED: &str = "f6bb3aa676b41c9d5748509dda66406e759d1b7fba5a83017cda88006b610282";

pub fn template_test(test_templates: &[&str]) -> TemplateTest {
    let Ok(wasm_path) = std::env::var("BURN_WALLET_WASM") else {
        return TemplateTest::new(".", std::iter::once(".").chain(test_templates.iter().copied()));
    };
    let code = std::fs::read(&wasm_path).unwrap_or_else(|e| panic!("BURN_WALLET_WASM {wasm_path}: {e}"));
    let address_hex = std::env::var("BURN_WALLET_TEMPLATE").unwrap_or_else(|_| PUBLISHED.to_string());
    let address = TemplateAddress::from_hex(address_hex.trim_start_matches("template_")).expect("template address");

    let mut builder = Package::builder();
    builder.add_all_builtin_templates();
    builder.add_template_from_code(address, code).expect("load the burn wallet binary");
    for path in test_templates {
        builder.add_template(path);
    }
    // Exactly what TemplateTest::new does after building its package.
    let mut test = TemplateTest::from_package(builder.build());
    test.bootstrap_state();
    test
}
