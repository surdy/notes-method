//! Durability tests: transcripts must survive a daemon restart, modelled here as
//! dropping the [`TranscriptStore`] and reopening the same on-disk database file.

use notesmith_transcript::{Role, TranscriptStore};

#[test]
fn transcripts_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcripts.sqlite");

    let thread_id = {
        let store = TranscriptStore::open(&path).unwrap();
        let t = store
            .create_thread("my-vault", "Planning", Some("copilot"), Some("gpt-5"))
            .unwrap();
        store
            .append_message("my-vault", &t.id, Role::User, "outline the week")
            .unwrap();
        store
            .append_message("my-vault", &t.id, Role::Agent, "here is a plan")
            .unwrap();
        t.id
    }; // store dropped — simulates daemon shutdown

    // Reopen the same file — simulates the daemon restarting.
    let store = TranscriptStore::open(&path).unwrap();
    let threads = store.list_threads("my-vault").unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, thread_id);
    assert_eq!(threads[0].title, "Planning");
    assert_eq!(threads[0].agent.as_deref(), Some("copilot"));
    assert_eq!(threads[0].model.as_deref(), Some("gpt-5"));

    let msgs = store.load_messages("my-vault", &thread_id).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].content, "outline the week");
    assert_eq!(msgs[1].content, "here is a plan");
}

#[test]
fn separate_vaults_share_one_db_without_leaking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcripts.sqlite");
    let store = TranscriptStore::open(&path).unwrap();

    let a = store
        .create_thread("alpha", "A thread", None, None)
        .unwrap();
    let b = store.create_thread("beta", "B thread", None, None).unwrap();
    store
        .append_message("alpha", &a.id, Role::User, "alpha-only")
        .unwrap();

    assert_eq!(store.list_threads("alpha").unwrap().len(), 1);
    assert_eq!(store.list_threads("beta").unwrap().len(), 1);
    assert!(store.get_thread("beta", &a.id).unwrap().is_none());
    assert!(store.load_messages("beta", &a.id).unwrap().is_empty());
    let _ = b;
}
