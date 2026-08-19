use encoding_rs::{
    BIG5,
    EUC_KR,
    GBK,
    SHIFT_JIS,
    UTF_8,
    WINDOWS_1250,
    WINDOWS_1251,
    WINDOWS_1252,
};

pub fn decode_rtf(bytes: &[u8]) -> String {
    if let Some(code_page) = detect_rtf_code_page(bytes) {
        if let Some(encoding) = encoding_from_code_page(code_page) {
            let (text, _, _) = encoding.decode(bytes);

            return text.into_owned();
        }
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }

    let (text, _, _) = WINDOWS_1252.decode(bytes);

    text.into_owned()
}

fn detect_rtf_code_page(bytes: &[u8]) -> Option<u16> {
    const MAX_HEADER_SIZE: usize = 4096;

    let header_len = bytes
        .len()
        .min(MAX_HEADER_SIZE);

    let header = &bytes[..header_len];

    let marker = b"\\ansicpg";

    let position = header
        .windows(marker.len())
        .position(|window| window == marker)?;

    let start = position + marker.len();

    let mut end = start;

    while end < header.len()
        && header[end].is_ascii_digit()
    {
        end += 1;
    }

    if start == end {
        return None;
    }

    std::str::from_utf8(&header[start..end])
        .ok()?
        .parse::<u16>()
        .ok()
}

fn encoding_from_code_page(
    code_page: u16,
) -> Option<&'static encoding_rs::Encoding> {
    match code_page {
        1250 => Some(WINDOWS_1250),
        1251 => Some(WINDOWS_1251),
        1252 => Some(WINDOWS_1252),

        932 => Some(SHIFT_JIS),
        936 => Some(GBK),
        949 => Some(EUC_KR),
        950 => Some(BIG5),

        _ => None,
    }
}
