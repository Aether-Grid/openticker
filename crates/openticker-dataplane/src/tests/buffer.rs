use super::bar;
use crate::StreamBuffer;

#[test]
fn older_bar_is_ignored_without_mutating_buffer() {
    let mut buffer = StreamBuffer::new(3);
    assert!(buffer.push_if_newer(bar(1, 100.0)));
    assert!(buffer.push_if_newer(bar(3, 102.0)));

    let before = buffer.snapshot(10);
    assert!(!buffer.push_if_newer(bar(2, 101.0)));
    let after = buffer.snapshot(10);
    assert_eq!(before, after);

    let closes = buffer
        .snapshot(10)
        .into_iter()
        .map(|bar| bar.close)
        .collect::<Vec<_>>();
    assert_eq!(closes, vec![100.0, 102.0]);
}
