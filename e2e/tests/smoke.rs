//! End-to-end browser smoke test driven by `fantoccini` (a Rust WebDriver client
//! — no node toolchain, matching the all-Rust repo).
//!
//! This is `#[ignore]` by default so a stray `cargo test` never tries to drive a
//! browser. Run it through `scripts/e2e.sh`, which brings up the dependency
//! stack, the server + workers, the Trunk-served frontend, and geckodriver
//! first. Selectors are intentionally resilient (by input type / visible text)
//! but may need tightening as the UI evolves.

use std::time::Duration;

use fantoccini::{ClientBuilder, Locator};

/// WebDriver endpoint (geckodriver default).
const WEBDRIVER: &str = "http://localhost:4444";
/// Trunk dev server (see crates/frontend/Trunk.toml).
const APP: &str = "http://127.0.0.1:8081";

/// Seeded credentials — keep in sync with `infra/postgres/*seed.sql`.
const EMAIL: &str = "director@portal.test";
const PASSWORD: &str = "portal-dev-password";

#[tokio::test]
#[ignore = "requires the full stack + geckodriver; run via scripts/e2e.sh"]
async fn login_then_raise_ticket_then_see_it_in_list() -> anyhow::Result<()> {
    let c = ClientBuilder::rustls()?.connect(WEBDRIVER).await?;
    let result = run(&c).await;
    // Always close the session so a panic doesn't leak the browser.
    c.close().await.ok();
    result
}

async fn run(c: &fantoccini::Client) -> anyhow::Result<()> {
    // 1. Log in.
    c.goto(&format!("{APP}/login")).await?;
    c.wait()
        .for_element(Locator::Css("input[type=email]"))
        .await?
        .send_keys(EMAIL)
        .await?;
    c.find(Locator::Css("input[type=password]"))
        .await?
        .send_keys(PASSWORD)
        .await?;
    c.find(Locator::Css("button[type=submit]"))
        .await?
        .click()
        .await?;

    // 2. Auth resolved -> the sidebar (authed shell) renders its tickets link.
    c.wait()
        .at_most(Duration::from_secs(10))
        .for_element(Locator::Css("a[href='/tickets']"))
        .await?
        .click()
        .await?;

    // 3. Raise a ticket with a unique marker so we can find it in the list.
    let marker = unique_marker();
    c.wait()
        .for_element(Locator::XPath("//button[contains(., 'Raise')]"))
        .await?
        .click()
        .await?;
    c.wait()
        .for_element(Locator::Css("input"))
        .await?
        .send_keys(&marker)
        .await?;
    c.find(Locator::XPath(
        "//button[@type='submit' or contains(., 'Submit') or contains(., 'Raise')]",
    ))
    .await?
    .click()
    .await?;

    // 4. The new ticket's marker appears in the list.
    c.wait()
        .at_most(Duration::from_secs(10))
        .for_element(Locator::XPath(&format!(
            "//*[contains(text(), '{marker}')]"
        )))
        .await?;

    Ok(())
}

/// A per-run unique ticket title. An e2e run genuinely needs a fresh marker; the
/// process id plus a monotonic counter is enough to disambiguate across reruns.
fn unique_marker() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!(
        "e2e-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}
