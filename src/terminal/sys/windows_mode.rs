//! Platform-independent console mode bit manipulation.

const ENABLE_MOUSE_MODE: u32 = 0x0010 | 0x0080 | 0x0008;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
pub(crate) const NOT_RAW_MODE_MASK: u32 = 0x0001 | 0x0002 | 0x0004;

/// Compute the console mode used by raw mode while preserving every other bit.
pub(crate) fn compute_enable_raw_mode(current: u32) -> u32 {
    current & !NOT_RAW_MODE_MASK
}

/// Compute the console mode used when leaving raw mode while preserving every other bit.
pub(crate) fn compute_disable_raw_mode(current: u32) -> u32 {
    current | NOT_RAW_MODE_MASK
}

/// Compute the console mode to apply when disabling mouse capture.
///
/// `current` — the mode read from the console handle immediately before the call.
/// `original` — the mode that was in effect before crossterm first touched it.
///
/// Restore the three mouse bits from the saved snapshot, as the previous
/// `set_mode(original)` did, while taking every unrelated bit from the current
/// mode. This preserves VT input and raw-mode bits that crossterm may have set.
pub(crate) fn compute_disable_mouse_capture_mode(current: u32, original: u32) -> u32 {
    (current & !ENABLE_MOUSE_MODE) | (original & ENABLE_MOUSE_MODE)
}

/// Restore only the VT-input bit from the first-touch mode snapshot.
pub(crate) fn compute_restore_vt_input_mode(current: u32, original: u32) -> u32 {
    (current & !ENABLE_VIRTUAL_TERMINAL_INPUT) | (original & ENABLE_VIRTUAL_TERMINAL_INPUT)
}

#[cfg(test)]
mod tests {
    use super::{
        NOT_RAW_MODE_MASK, compute_disable_mouse_capture_mode, compute_disable_raw_mode,
        compute_enable_raw_mode, compute_restore_vt_input_mode,
    };

    const ENABLE_MOUSE_INPUT: u32 = 0x0010;
    const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
    const ENABLE_WINDOW_INPUT: u32 = 0x0008;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    const RAW_MODE_BITS: u32 = 0x0001 | 0x0002 | 0x0004;
    const UNRELATED_BITS: u32 = 0x4000;
    const ALL_MOUSE_BITS: u32 = ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT;

    #[test]
    fn test_compute_restore_vt_input_mode_preserves_all_other_bits() {
        let current = 0x4000 | ENABLE_MOUSE_INPUT | ENABLE_VIRTUAL_TERMINAL_INPUT;
        let original = ENABLE_EXTENDED_FLAGS;
        let result = compute_restore_vt_input_mode(current, original);
        assert_eq!(result, 0x4000 | ENABLE_MOUSE_INPUT);
    }

    #[test]
    fn test_compute_restore_vt_input_mode_restores_original_vt_state() {
        let current = 0x4000;
        assert_eq!(
            compute_restore_vt_input_mode(current, ENABLE_VIRTUAL_TERMINAL_INPUT),
            current | ENABLE_VIRTUAL_TERMINAL_INPUT
        );
        assert_eq!(
            compute_restore_vt_input_mode(current | ENABLE_VIRTUAL_TERMINAL_INPUT, 0),
            current
        );
    }

    #[test]
    fn test_raw_mode_only_changes_raw_bits() {
        let current =
            ENABLE_VIRTUAL_TERMINAL_INPUT | ALL_MOUSE_BITS | RAW_MODE_BITS | UNRELATED_BITS;
        let enabled = compute_enable_raw_mode(current);
        assert_eq!(
            enabled,
            ENABLE_VIRTUAL_TERMINAL_INPUT | ALL_MOUSE_BITS | UNRELATED_BITS
        );
        let disabled = compute_disable_raw_mode(enabled);
        assert_eq!(
            disabled,
            ENABLE_VIRTUAL_TERMINAL_INPUT | ALL_MOUSE_BITS | RAW_MODE_BITS | UNRELATED_BITS
        );
    }

    #[test]
    fn test_raw_and_paste_transforms_compose_in_either_order() {
        let original_off = NOT_RAW_MODE_MASK | ALL_MOUSE_BITS | UNRELATED_BITS;
        let original_on = original_off | ENABLE_VIRTUAL_TERMINAL_INPUT;

        for original in [original_off, original_on] {
            // raw -> paste -> disable raw -> restore paste
            let raw_then_paste = compute_enable_raw_mode(original) | ENABLE_VIRTUAL_TERMINAL_INPUT;
            let raw_then_paste = compute_disable_raw_mode(raw_then_paste);
            assert_eq!(
                compute_restore_vt_input_mode(raw_then_paste, original),
                original
            );

            // paste -> raw -> restore paste -> disable raw
            let paste_then_raw = compute_enable_raw_mode(original | ENABLE_VIRTUAL_TERMINAL_INPUT);
            let paste_then_raw = compute_restore_vt_input_mode(paste_then_raw, original);
            assert_eq!(compute_disable_raw_mode(paste_then_raw), original);

            // A paste disable/re-enable cycle restores the same snapshot.
            let repasted = compute_restore_vt_input_mode(
                compute_restore_vt_input_mode(
                    compute_enable_raw_mode(original) | ENABLE_VIRTUAL_TERMINAL_INPUT,
                    original,
                ) | ENABLE_VIRTUAL_TERMINAL_INPUT,
                original,
            );
            assert_eq!(repasted, compute_enable_raw_mode(original));
            assert_eq!(compute_disable_raw_mode(repasted), original);
        }
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_clears_mouse_bits_missing_from_original() {
        let current = ALL_MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT;

        let result = compute_disable_mouse_capture_mode(current, 0);

        assert_eq!(result, ENABLE_VIRTUAL_TERMINAL_INPUT);
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_preserves_original_window_input() {
        let current = ALL_MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT;

        let result = compute_disable_mouse_capture_mode(current, ENABLE_WINDOW_INPUT);

        assert_eq!(result, ENABLE_WINDOW_INPUT | ENABLE_VIRTUAL_TERMINAL_INPUT);
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_preserves_all_original_mouse_bits() {
        let current = ALL_MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT;

        let result = compute_disable_mouse_capture_mode(current, ALL_MOUSE_BITS);

        assert_eq!(result, current);
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_preserves_mixed_original_mouse_bits() {
        let current = ALL_MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT;
        let original = ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT;

        let result = compute_disable_mouse_capture_mode(current, original);

        assert_eq!(result, original | ENABLE_VIRTUAL_TERMINAL_INPUT);
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_restores_mouse_bit_lost_from_current() {
        let current = ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT | ENABLE_VIRTUAL_TERMINAL_INPUT;
        let original = ENABLE_MOUSE_INPUT;

        let result = compute_disable_mouse_capture_mode(current, original);

        assert_eq!(result, ENABLE_MOUSE_INPUT | ENABLE_VIRTUAL_TERMINAL_INPUT);
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_preserves_raw_mode_and_unrelated_bits() {
        let current =
            ALL_MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT | RAW_MODE_BITS | UNRELATED_BITS;

        let result = compute_disable_mouse_capture_mode(current, 0);

        assert_eq!(
            result,
            ENABLE_VIRTUAL_TERMINAL_INPUT | RAW_MODE_BITS | UNRELATED_BITS
        );
    }

    #[test]
    fn test_compute_disable_mouse_capture_mode_exhaustive_mouse_bits() {
        let non_mouse_bits = ENABLE_VIRTUAL_TERMINAL_INPUT | RAW_MODE_BITS | UNRELATED_BITS;

        for current_combination in 0..8 {
            let current_mouse_bits = (0..3).fold(0, |bits, index| {
                if current_combination & (1 << index) != 0 {
                    bits | [
                        ENABLE_MOUSE_INPUT,
                        ENABLE_EXTENDED_FLAGS,
                        ENABLE_WINDOW_INPUT,
                    ][index]
                } else {
                    bits
                }
            });
            let current = current_mouse_bits | non_mouse_bits;

            for original_combination in 0..8 {
                let original_mouse_bits = (0..3).fold(0, |bits, index| {
                    if original_combination & (1 << index) != 0 {
                        bits | [
                            ENABLE_MOUSE_INPUT,
                            ENABLE_EXTENDED_FLAGS,
                            ENABLE_WINDOW_INPUT,
                        ][index]
                    } else {
                        bits
                    }
                });

                let result = compute_disable_mouse_capture_mode(current, original_mouse_bits);

                assert_eq!(
                    result & ALL_MOUSE_BITS,
                    original_mouse_bits,
                    "mouse bits must match original: current={current_combination:#05b}, original={original_combination:#05b}"
                );
                assert_eq!(
                    result & !ALL_MOUSE_BITS,
                    current & !ALL_MOUSE_BITS,
                    "non-mouse bits must match current: current={current_combination:#05b}, original={original_combination:#05b}"
                );
            }
        }
    }
}
