use super::*;

fn page(ids: impl IntoIterator<Item = u64>, total: u64) -> Value {
    json!({"totalNum":total,"songList":ids.into_iter().map(|id|json!({"songInfo":{"id":id}})).collect::<Vec<_>>()})
}
fn read(pages: Vec<Result<Value>>) -> Result<Vec<Value>> {
    let mut pages = pages.into_iter();
    let result = collect_album(|_| pages.next().expect("unexpected extra page request"));
    assert!(pages.next().is_none(), "all fixture pages should have been requested");
    result
}

#[test]
fn normal_pages_use_actual_offsets_and_preserve_order_and_occurrences() {
    let first = page(0..100, 203);
    let second = first.clone(); // Identical pages can represent legitimate repeated entries.
    let last = json!({"totalNum":203,"songList":[{"id":5},{"songInfo":{"id":0}},{"id":5}]});
    let mut pages = [(0, first), (100, second), (200, last)].into_iter();
    let tracks = collect_album(|begin| {
        let (expected, page) = pages.next().expect("unexpected extra request");
        assert_eq!(begin, expected);
        Ok(page)
    }).unwrap();
    assert!(pages.next().is_none());
    let expected = (0..100).chain(0..100).chain([5, 0, 5]).map(|id|json!({"id":id})).collect::<Vec<_>>();
    assert_eq!(tracks, expected);
}

#[test]
fn empty_album_and_exact_page_boundary_complete_without_an_extra_request() {
    assert!(read(vec![Ok(page([], 0))]).unwrap().is_empty());
    assert_eq!(read(vec![Ok(page(0..100, 100))]).unwrap().len(), 100);
    // A short non-empty page does not imply EOF; begin follows the received count.
    assert_eq!(read(vec![Ok(page(0..2, 3)), Ok(page(2..3, 3))]).unwrap().len(), 3);
}

#[test]
fn changed_total_is_rejected_even_when_the_new_count_would_appear_complete() {
    for second in [page([], 100), page(100..200, 200), page(100..150, 149)] {
        let error = read(vec![Ok(page(0..100, 150)), Ok(second)]).unwrap_err();
        assert!(error.to_string().contains("总数发生变化"));
    }
}

#[test]
fn excess_tracks_are_rejected_without_deduplicating_the_response() {
    for pages in [vec![Ok(page(0..100, 150)), Ok(page(0..100, 150))], vec![Ok(page(0..1, 0))]] {
        let error = read(pages).unwrap_err();
        assert!(error.to_string().contains("数量不一致"));
    }
}

#[test]
fn empty_incomplete_page_and_midstream_failure_never_return_partial_success() {
    let error = read(vec![Ok(page(0..100, 150)), Ok(page([], 150))]).unwrap_err();
    assert!(error.to_string().contains("未取全"));
    let error = read(vec![Ok(page(0..100, 150)), Err(anyhow::anyhow!("synthetic network failure"))]).unwrap_err();
    assert_eq!(error.to_string(), "synthetic network failure");
}

#[test]
fn missing_fields_and_page_limit_are_errors() {
    for page in [json!({"totalNum":0}), json!({"songList":[]}), json!({"songList":[],"totalNum":-1})] {
        assert!(read(vec![Ok(page)]).is_err());
    }
    let mut calls = 0;
    let error = collect_album(|begin| {
        assert_eq!(begin, calls);
        calls += 1;
        Ok(page([7], 41)) // Legal duplicates cannot bypass the existing page limit.
    }).unwrap_err();
    assert_eq!(calls, 40);
    assert!(error.to_string().contains("分页上限"));
}
