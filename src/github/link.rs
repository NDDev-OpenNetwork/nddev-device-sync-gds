//! Bounded Link syntax for this one GitHub continuation. Separators inside
//! URI/quoted values are not list boundaries; ambiguous input is never EOF.
use nddev_device_sync_domain::modules::ModuleError;

pub(super) fn next_target(header: &str) -> Result<Option<&str>, ModuleError> {
    let bytes = header.as_bytes();
    if bytes.is_empty() || bytes.len() > 8192 {
        return Err(ModuleError::InvalidResponse);
    }
    let mut index = 0;
    let mut next = None;
    loop {
        ows(bytes, &mut index);
        if bytes.get(index) != Some(&b'<') {
            return Err(ModuleError::InvalidResponse);
        }
        index += 1;
        let start = index;
        while let Some(&byte) = bytes.get(index) {
            if byte == b'>' {
                break;
            }
            if byte <= b' ' || byte == b'<' || byte == 127 {
                return Err(ModuleError::InvalidResponse);
            }
            index += 1;
        }
        if bytes.get(index) != Some(&b'>') {
            return Err(ModuleError::InvalidResponse);
        }
        let target = &header[start..index];
        index += 1;
        let mut relations = None;
        let mut anchored = false;
        loop {
            ows(bytes, &mut index);
            match bytes.get(index) {
                None | Some(b',') => break,
                Some(b';') => index += 1,
                _ => return Err(ModuleError::InvalidResponse),
            }
            ows(bytes, &mut index);
            let start = index;
            while bytes.get(index).is_some_and(|byte| token(*byte)) {
                index += 1;
            }
            if start == index {
                return Err(ModuleError::InvalidResponse);
            }
            let name = &header[start..index];
            ows(bytes, &mut index);
            let value = if bytes.get(index) == Some(&b'=') {
                index += 1;
                ows(bytes, &mut index);
                Some(value(bytes, &mut index)?)
            } else {
                None
            };
            if name.eq_ignore_ascii_case("rel") {
                if relations.is_some() {
                    return Err(ModuleError::InvalidResponse);
                }
                relations = Some(value.ok_or(ModuleError::InvalidResponse)?);
            }
            if name.eq_ignore_ascii_case("anchor") {
                anchored = true;
            }
        }
        let relations = relations.ok_or(ModuleError::InvalidResponse)?;
        if relations.is_empty()
            || relations.starts_with(' ')
            || relations.ends_with(' ')
            || relations.contains('\t')
        {
            return Err(ModuleError::InvalidResponse);
        }
        let mut is_next = false;
        for relation in relations.split(' ').filter(|value| !value.is_empty()) {
            let registered = relation
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && relation
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'));
            if !registered && url::Url::parse(relation).is_err() {
                return Err(ModuleError::InvalidResponse);
            }
            if relation.eq_ignore_ascii_case("next") {
                if is_next {
                    return Err(ModuleError::InvalidResponse);
                }
                is_next = true;
            }
        }
        if is_next {
            if anchored || next.is_some() {
                return Err(ModuleError::InvalidResponse);
            }
            next = Some(target);
        }
        if index == bytes.len() {
            return Ok(next);
        }
        // The comma belongs to the list only after the whole URI/parameter set.
        index += 1;
        if index == bytes.len() {
            return Err(ModuleError::InvalidResponse);
        }
    }
}
fn ows(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        *index += 1;
    }
}
fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}
fn value(bytes: &[u8], index: &mut usize) -> Result<String, ModuleError> {
    if bytes.get(*index) != Some(&b'"') {
        let start = *index;
        while bytes.get(*index).is_some_and(|byte| token(*byte)) {
            *index += 1;
        }
        if start == *index {
            return Err(ModuleError::InvalidResponse);
        }
        return String::from_utf8(bytes[start..*index].to_vec())
            .map_err(|_| ModuleError::InvalidResponse);
    }
    *index += 1;
    let mut output = Vec::new();
    loop {
        let mut byte = *bytes.get(*index).ok_or(ModuleError::InvalidResponse)?;
        *index += 1;
        if byte == b'"' {
            return String::from_utf8(output).map_err(|_| ModuleError::InvalidResponse);
        }
        if byte == b'\\' {
            byte = *bytes.get(*index).ok_or(ModuleError::InvalidResponse)?;
            *index += 1;
        }
        if byte < b' ' && byte != b'\t' || byte == 127 {
            return Err(ModuleError::InvalidResponse);
        }
        output.push(byte);
    }
}
