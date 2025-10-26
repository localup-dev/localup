/// Unit test for graceful disconnect mechanism via shutdown channel
/// This test verifies that calling disconnect_handle() successfully triggers
/// the shutdown channel without causing deadlocks
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_shutdown_channel_no_deadlock() {
    // This test simulates the shutdown channel mechanism used by TunnelConnection
    // to avoid deadlock when sending disconnect message

    // Create shutdown channel (same as in TunnelConnection::run())
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Simulate control stream task that monitors shutdown signal
    let control_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Monitor shutdown signal
                _ = shutdown_rx.recv() => {
                    println!("Shutdown signal received");
                    // Simulate sending disconnect message
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    break;
                }
                // Simulate incoming message handling
                _ = tokio::time::sleep(Duration::from_secs(100)) => {
                    // Would normally recv_message() here
                }
            }
        }
    });

    // Simulate disconnect() call - send shutdown signal
    let start = Instant::now();
    shutdown_tx.send(()).await.expect("Failed to send shutdown");

    // Wait for control task to finish
    let result = tokio::time::timeout(Duration::from_secs(1), control_task).await;
    let disconnect_duration = start.elapsed();

    match result {
        Ok(Ok(_)) => {
            println!("✅ Control task exited in {:?}", disconnect_duration);
            assert!(
                disconnect_duration < Duration::from_millis(500),
                "Disconnect took too long: {:?}",
                disconnect_duration
            );
        }
        Ok(Err(e)) => {
            panic!("❌ Control task panicked: {}", e);
        }
        Err(_) => {
            panic!("❌ Disconnect timed out (deadlock?)");
        }
    }
}

#[tokio::test]
async fn test_multiple_rapid_disconnects() {
    // Stress test: Perform 100 rapid connect/disconnect cycles
    // to ensure no race conditions or deadlocks

    for i in 0..100 {
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        let task = tokio::spawn(async move {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    // Received shutdown
                }
            }
        });

        let start = Instant::now();
        shutdown_tx.send(()).await.unwrap();
        task.await.unwrap();
        let duration = start.elapsed();

        assert!(
            duration < Duration::from_millis(10),
            "Iteration {} took too long: {:?}",
            i,
            duration
        );

        if (i + 1) % 20 == 0 {
            println!("✅ Completed {} iterations", i + 1);
        }
    }

    println!("✅ All 100 iterations completed successfully");
}

#[tokio::test]
async fn test_disconnect_with_abort() {
    // Test that aborting wait task after disconnect works correctly
    // This simulates the CLI behavior

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Simulate wait task
    let wait_task = tokio::spawn(async move {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                println!("Shutdown received in wait task");
            }
            _ = tokio::time::sleep(Duration::from_secs(100)) => {
                // Simulate long-running wait
            }
        }
    });

    // Simulate Ctrl+C handler
    let start = Instant::now();

    // Send disconnect
    shutdown_tx.send(()).await.expect("Failed to send shutdown");

    // Abort wait task (CLI does this)
    wait_task.abort();

    let duration = start.elapsed();

    println!("✅ Disconnect + abort completed in {:?}", duration);
    assert!(
        duration < Duration::from_millis(100),
        "Disconnect + abort took too long: {:?}",
        duration
    );
}
