/// One access unit (a whole frame) of an Annex-B H.264 stream.
pub struct AccessUnit<'a> {
    pub data: &'a [u8],
    pub key: bool,
}

const ACCESS_UNIT_DELIMITER: u8 = 9;
const IDR_SLICE: u8 = 5;

/// Byte offsets of every NAL unit's start code, with the NAL unit type after it.
fn nal_starts(stream: &[u8]) -> Vec<(usize, u8)> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 < stream.len() {
        if stream[index] != 0 || stream[index + 1] != 0 {
            index += 1;
            continue;
        }
        let code_length = match (stream[index + 2], stream.get(index + 3)) {
            (1, _) => 3,
            (0, Some(1)) => 4,
            _ => {
                index += 1;
                continue;
            }
        };
        let header = index + code_length;
        if header < stream.len() {
            starts.push((index, stream[header] & 0x1f));
        }
        index = header;
    }
    starts
}

/// Splits a stream into frames at its access unit delimiters, the way `capture.ts`
/// writes them and `ffmpeg -x264-params aud=1` encodes them.
pub fn split_access_units(stream: &[u8]) -> Result<Vec<AccessUnit<'_>>, String> {
    let starts = nal_starts(stream);
    let delimiters: Vec<usize> = starts
        .iter()
        .filter(|(_, kind)| *kind == ACCESS_UNIT_DELIMITER)
        .map(|(offset, _)| *offset)
        .collect();
    if delimiters.is_empty() {
        return Err(
            "stream has no access unit delimiters (record it with capture.ts or encode with aud=1)"
                .into(),
        );
    }
    let mut units = Vec::with_capacity(delimiters.len());
    for (position, start) in delimiters.iter().enumerate() {
        let end = delimiters
            .get(position + 1)
            .copied()
            .unwrap_or(stream.len());
        let key = starts
            .iter()
            .any(|(offset, kind)| *offset >= *start && *offset < end && *kind == IDR_SLICE);
        units.push(AccessUnit {
            data: &stream[*start..end],
            key,
        });
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_delimiters_and_finds_idr() {
        let stream = [
            0, 0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x65, 2, 0, 0, 0, 1, 9, 0xf0, 0, 0,
            1, 0x41, 3,
        ];
        let units = split_access_units(&stream).unwrap();
        assert_eq!(units.len(), 2);
        assert!(units[0].key);
        assert!(!units[1].key);
        assert_eq!(units[1].data, &stream[17..]);
    }

    #[test]
    fn rejects_stream_without_delimiters() {
        assert!(split_access_units(&[0, 0, 0, 1, 0x65, 1]).is_err());
    }
}
