//! Explicit test-only MCP host: real transport/application requests, no camera or desktop window.
#[test]
#[ignore = "Requires an owned test port and stop-file harness; never uses the user instance"]
fn owned_headless_mcp_host() {
    use std::{path::PathBuf, time::{Duration, Instant}};
    let port = std::env::var("DPER_MEASURE_MCP_PORT").expect("dedicated test port");
    assert!(port.parse::<u16>().is_ok_and(|p| p >= 18765));
    let stop = PathBuf::from(std::env::var("DPER_TEST_HOST_STOP").expect("owned stop path"));
    assert!(!stop.exists());
    let ctx = egui::Context::default();
    let mut app = crate::DperMeasureApp::new_with_services(ctx.clone(), false);
    app.mcp_server = Some(crate::mcp_server::start(app.mcp_bridge.clone()).expect("MCP startup"));
    let started = Instant::now();
    let mut observed_running = false;
    while !stop.exists() && started.elapsed() < Duration::from_secs(180) {
        match app.mcp_server.as_ref().unwrap().status() {
            crate::mcp_server::ServerStatus::Running if !observed_running => {
                observed_running = true;
                eprintln!("OWNED_MCP_RUNNING port={port}");
            }
            crate::mcp_server::ServerStatus::Failed(error) => panic!("MCP startup failed: {error}"),
            _ => {}
        }
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.handle_mcp_requests(ctx);
            app.poll_drawing_geometry_analysis(ctx);
        });
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(stop.exists(), "MCP harness did not finish before timeout");
    assert!(observed_running, "MCP host never reached running state");
    assert!(app.document_revision > 1, "No mutating application scenario was exercised");
}
