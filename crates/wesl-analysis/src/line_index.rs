#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// How a client counts columns. UTF-16 is the protocol default and every client supports it;
/// UTF-8 is worth negotiating because this crate is byte-offset native, so it removes the
/// conversion entirely rather than replacing it with a different one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionEncoding {
    #[default]
    Utf16,
    Utf8,
}

#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    encoding: PositionEncoding,
}

impl LineIndex {
    pub fn new(source: &str, encoding: PositionEncoding) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
        );
        Self {
            line_starts,
            encoding,
        }
    }

    pub fn offset_to_position(&self, source: &str, offset: usize) -> Option<Position> {
        if offset > source.len() || !source.is_char_boundary(offset) {
            return None;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let character = match self.encoding {
            PositionEncoding::Utf8 => offset - line_start,
            PositionEncoding::Utf16 => source[line_start..offset].encode_utf16().count(),
        };
        Some(Position {
            line: line.try_into().ok()?,
            character: character.try_into().ok()?,
        })
    }

    /// Converts an LSP position to a byte offset in `source`.
    ///
    /// Out-of-range positions are clamped rather than rejected, matching the LSP spec: a
    /// `character` past the end of its line clamps to the line end, and a `line` past the
    /// last line clamps to the end of the document. `None` is reserved for a position that is
    /// genuinely malformed — a column landing inside a multi-byte UTF-8 character or a
    /// UTF-16 surrogate pair, which has no addressable byte offset.
    pub fn position_to_offset(&self, source: &str, position: Position) -> Option<usize> {
        let Some(&line_start) = self.line_starts.get(position.line as usize) else {
            // No such line: clamp to end of document rather than rejecting.
            return Some(source.len());
        };
        let next_line_start = self.line_starts.get(position.line as usize + 1).copied();
        let line_end = next_line_start.map_or(source.len(), |next| next.saturating_sub(1));
        // `line_end` is the index of the line terminator (`\n` when a next line exists).
        // On a CRLF document that index still includes the preceding `\r`, so clamping an
        // oversized character straight to `line_end` would land the offset inside the
        // two-byte terminator; `apply_content_changes` would then consume the `\r` but
        // leave the `\n` behind, silently desyncing the buffer from the client. The LSP
        // spec's line length excludes the terminator, so strip a trailing `\r` from the
        // clamp target here — both encoding branches below share this single value so they
        // can't disagree with each other.
        let content_end = if next_line_start.is_some()
            && source.as_bytes().get(line_end.wrapping_sub(1)) == Some(&b'\r')
        {
            line_end - 1
        } else {
            line_end
        };
        let target = position.character as usize;

        if self.encoding == PositionEncoding::Utf8 {
            let offset = line_start + target;
            if offset >= content_end {
                // Character past the line (or exactly at its end) clamps to the line end.
                return Some(content_end);
            }
            // A column landing inside a multi-byte character is still rejected as malformed.
            return source.is_char_boundary(offset).then_some(offset);
        }

        let line = &source[line_start..content_end];
        let mut utf16_col = 0;
        for (byte_offset, ch) in line.char_indices() {
            if utf16_col == target {
                return Some(line_start + byte_offset);
            }
            utf16_col += ch.len_utf16();
            if utf16_col > target {
                // Landed inside a surrogate pair: genuinely malformed, not just out of range.
                return None;
            }
        }
        // Ran out of line before reaching the target column: clamp to the line end.
        Some(content_end)
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position, PositionEncoding};

    #[test]
    fn positions_use_utf8_columns_when_negotiated() {
        let source = "a😀b\néx";
        let index = LineIndex::new(source, PositionEncoding::Utf8);

        // Under UTF-8 the column is the byte offset within the line, so the emoji counts 4.
        let b = source.find('b').unwrap();
        assert_eq!(
            index.offset_to_position(source, b),
            Some(Position {
                line: 0,
                character: 5,
            })
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 5,
                },
            ),
            Some(b)
        );

        // A column landing inside the emoji is still rejected rather than silently rounded.
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 3,
                },
            ),
            None
        );

        // Round-trips for every boundary on the second line.
        let line_start = source.find('é').unwrap();
        for offset in [line_start, line_start + 2, source.len()] {
            let position = index.offset_to_position(source, offset).unwrap();
            assert_eq!(index.position_to_offset(source, position), Some(offset));
        }
    }

    #[test]
    fn positions_use_utf16_columns() {
        let source = "a😀b\néx";
        let index = LineIndex::new(source, PositionEncoding::Utf16);
        let b = source.find('b').unwrap();
        assert_eq!(
            index.offset_to_position(source, b),
            Some(Position {
                line: 0,
                character: 3,
            })
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 3,
                },
            ),
            Some(b)
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 2,
                },
            ),
            None
        );
    }

    #[test]
    fn position_to_offset_clamps_out_of_range_character_and_line() {
        let source = "a😀b\néx";
        let line_end = source.find('\n').unwrap();
        let doc_end = source.len();

        // A character far past the end of a line clamps to that line's end, per the LSP spec,
        // rather than being rejected as if it were malformed.
        let utf8_index = LineIndex::new(source, PositionEncoding::Utf8);
        assert_eq!(
            utf8_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 999,
                },
            ),
            Some(line_end)
        );
        let utf16_index = LineIndex::new(source, PositionEncoding::Utf16);
        assert_eq!(
            utf16_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 999,
                },
            ),
            Some(line_end)
        );

        // A line far past the last line clamps to the end of the document.
        assert_eq!(
            utf8_index.position_to_offset(
                source,
                Position {
                    line: 999,
                    character: 0,
                },
            ),
            Some(doc_end)
        );
        assert_eq!(
            utf16_index.position_to_offset(
                source,
                Position {
                    line: 999,
                    character: 999,
                },
            ),
            Some(doc_end)
        );

        // Mid-character columns remain genuinely malformed, not merely out of range.
        assert_eq!(
            utf8_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 3,
                },
            ),
            None
        );
        assert_eq!(
            utf16_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 2,
                },
            ),
            None
        );
    }

    #[test]
    fn position_to_offset_clamps_crlf_before_the_terminator() {
        // "ab\r\ncd": line 0 is "ab", terminated by "\r\n" at bytes 2-3, line 1 starts at byte 4.
        let source = "ab\r\ncd";

        // A character far past the end of a CRLF-terminated line clamps to the content end
        // (offset 2, right after "ab"), not to the index of the "\n" (offset 3) — landing
        // there would leave the clamp inside the two-byte terminator and let
        // `apply_content_changes` consume the "\r" while leaving the "\n" behind.
        let utf8_index = LineIndex::new(source, PositionEncoding::Utf8);
        assert_eq!(
            utf8_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 999,
                },
            ),
            Some(2)
        );
        let utf16_index = LineIndex::new(source, PositionEncoding::Utf16);
        assert_eq!(
            utf16_index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 999,
                },
            ),
            Some(2)
        );

        // The clamped offset round-trips back to the same position in both encodings.
        let clamped = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(utf8_index.offset_to_position(source, 2), Some(clamped));
        assert_eq!(utf16_index.offset_to_position(source, 2), Some(clamped));

        // LF-only documents are unaffected: the clamp still lands on the index of "\n".
        let lf_source = "ab\ncd";
        let lf_index = LineIndex::new(lf_source, PositionEncoding::Utf8);
        assert_eq!(
            lf_index.position_to_offset(
                lf_source,
                Position {
                    line: 0,
                    character: 999,
                },
            ),
            Some(2)
        );
    }
}
