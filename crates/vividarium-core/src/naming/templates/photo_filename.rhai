// The backend normalizes every returned name again.
// normalize_name(value) applies the same shared normalization explicitly.

fn is_ascii_alpha(character) {
    (character >= 'A' && character <= 'Z')
        || (character >= 'a' && character <= 'z')
}

fn normalize_quotes(value) {
    let output = value;
    output.replace("\u2018", "'");
    output.replace("\u2019", "'");
    output.replace("\u201c", "'");
    output.replace("\u201d", "'");
    output.replace("\uff07", "'");
    output
}

fn filename_only(value) {
    let output = value;
    output.replace("\\", "/");
    let start = 0;
    for index in 0..output.len {
        if output.get(index) == '/' {
            start = index + 1;
        }
    }
    output.sub_string(start)
}

fn apostrophe_is_internal(value, index) {
    if index + 1 >= value.len {
        return false;
    }
    let next = value.get(index + 1);
    is_ascii_alpha(next)
        && next != 'M'
        && next != 'F'
        && !(next == 'Y'
            && index + 2 < value.len
            && value.get(index + 2) == 'N')
}

fn split_filename(value) {
    let quoted = false;
    let cutoff = value.len;
    for index in 0..value.len {
        let character = value.get(index);
        if character == '\'' {
            if quoted && apostrophe_is_internal(value, index) {
                continue;
            }
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if character >= '0' && character <= '9' {
            cutoff = index;
            break;
        }
        if character == '.'
            && (index + 1 >= value.len || value.get(index + 1) != ' ')
        {
            cutoff = index;
            break;
        }
    }
    if quoted {
        throw "photo filename contains an unmatched quote";
    }

    let information = value.sub_string(0, cutoff);
    let suffix = value.sub_string(cutoff);
    if information.ends_with("YN") {
        information = information.sub_string(0, information.len - 2);
        suffix = "YN" + suffix;
    } else if information.ends_with("M") || information.ends_with("F") {
        suffix = information.sub_string(information.len - 1) + suffix;
        information = information.sub_string(0, information.len - 1);
    }
    information.trim();
    #{ information: information, suffix: suffix }
}

fn split_chinese_scientific(value) {
    if value == "" || value.get(0) <= '\u007f' {
        return #{ chinese: "", scientific: value };
    }
    let end = 0;
    let index = 0;
    while index < value.len {
        let character = value.get(index);
        if character == '\'' || character == '(' {
            let closing = if character == '\'' { '\'' } else { ')' };
            end = index + 1;
            index += 1;
            while index < value.len {
                end = index + 1;
                if value.get(index) == closing {
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if character > '\u007f' {
            end = index + 1;
            index += 1;
            continue;
        }
        break;
    }
    let scientific = value.sub_string(end);
    scientific.trim();
    #{
        chinese: value.sub_string(0, end),
        scientific: scientific
    }
}

fn empty_info() {
    #{
        family_sci: (),
        genus_sci: (),
        species_sci: (),
        family_zh: (),
        genus_zh: (),
        species_zh: ()
    }
}

fn parse_chinese(value) {
    let output = empty_info();
    let family_index = value.index_of("\u79d1");
    if family_index >= 0 {
        let family_end = family_index + 1;
        if !value.sub_string(family_end).starts_with("\u79d1") {
            output.family_zh = normalize_name(value.sub_string(0, family_end));
            value = value.sub_string(family_end);
            value.trim();
        }
    }
    let genus_index = value.index_of("\u5c5e");
    if genus_index >= 0 {
        let genus_end = genus_index + 1;
        output.genus_zh = normalize_name(value.sub_string(0, genus_end));
        value = value.sub_string(genus_end);
        value.trim();
    }
    if value != "" {
        output.species_zh = normalize_name(value);
    }
    output
}

fn parse_scientific(value) {
    let output = empty_info();
    value = normalize_name(value);
    if value == "" {
        return output;
    }

    let last_separator = -1;
    for index in 0..value.len {
        if value.get(index) == ' ' {
            last_separator = index;
        }
    }
    if last_separator < 0 {
        if (value.len > 6 && value.ends_with("aceae"))
            || (value.len > 5 && value.ends_with("idae"))
        {
            output.family_sci = value;
        } else {
            output.genus_sci = value;
        }
    } else if value.sub_string(0, last_separator) == "x" {
        output.genus_sci = value;
    } else {
        output.genus_sci = normalize_name(value.sub_string(0, last_separator));
        output.species_sci = value;
    }
    output
}

fn parse_photo_filename(filename) {
    let filename = normalize_quotes(filename_only(filename));
    let parts = split_filename(filename);
    let information = normalize_name(parts.information);
    let sections = split_chinese_scientific(information);
    let chinese = parse_chinese(sections.chinese);
    let scientific = parse_scientific(sections.scientific);
    #{
        info: #{
            family_sci: scientific.family_sci,
            genus_sci: scientific.genus_sci,
            species_sci: scientific.species_sci,
            family_zh: chinese.family_zh,
            genus_zh: chinese.genus_zh,
            species_zh: chinese.species_zh
        },
        suffix: parts.suffix
    }
}
