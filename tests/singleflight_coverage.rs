use greentic_flow::cache::ArtifactKey;
use greentic_flow::cache::singleflight::Singleflight;
use tokio::sync::oneshot;

#[tokio::test]
async fn singleflight_serializes_same_key_and_releases_after_drop() {
    let singleflight = Singleflight::new();
    let key = ArtifactKey::new("profile".to_string(), "sha256:test".to_string());

    let guard = singleflight.acquire(key.clone()).await;
    let cloned = singleflight.clone();
    let (entered_tx, mut entered_rx) = oneshot::channel();

    let waiter = tokio::spawn(async move {
        let _guard = cloned.acquire(key).await;
        let _ = entered_tx.send(());
    });

    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(
        matches!(entered_rx.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty)),
        "second acquire should block while first guard is held"
    );

    drop(guard);

    waiter.await.expect("waiter should finish once the guard drops");
}
