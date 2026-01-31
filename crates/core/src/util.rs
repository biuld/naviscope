pub fn utf16_col_to_byte_col(content: &str, line: usize, utf16_col: usize) -> usize {
    let line_content = content.lines().nth(line).unwrap_or("");
    let mut curr_utf16 = 0;
    let mut curr_byte = 0;

    for c in line_content.chars() {
        if curr_utf16 >= utf16_col {
            break;
        }
        curr_utf16 += c.len_utf16();
        curr_byte += c.len_utf8();
    }
    curr_byte
}
