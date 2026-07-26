// input is the exact synonym string supplied by the user.
// The backend normalizes the returned name after this function completes.

fn split_synonym_authority(input) {
    let value = input;
    value.trim();
    if value == "" {
        return #{ name: "", authority_year: () };
    }

    let index = 0;
    let uppercase_words = 0;
    while index < value.len {
        while index < value.len && is_whitespace(value.get(index)) {
            index += 1;
        }
        if index >= value.len {
            break;
        }
        let start = index;
        while index < value.len && !is_whitespace(value.get(index)) {
            index += 1;
        }
        let word = value.sub_string(start, index - start);
        let matched = word.contains("(")
            || word == "'t"
            || word == "de"
            || word == "von"
            || word == "van";
        if !matched {
            if is_uppercase(word.get(0)) {
                uppercase_words += 1;
                matched = uppercase_words >= 2;
            }
        }
        if matched {
            return #{
                name: value.sub_string(0, start),
                authority_year: value.sub_string(start)
            };
        }
    }
    #{ name: value, authority_year: () }
}
