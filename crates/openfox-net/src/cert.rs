//! Certificate inspection: a minimal DER walker that pulls the fields a
//! certificate viewer actually shows — subject, issuer, validity window,
//! SANs, serial — plus the error taxonomy the interstitial speaks.

#[derive(Debug, Clone, PartialEq)]
pub struct CertSummary {
    pub subject_cn: String,
    pub issuer_cn: String,
    pub serial_hex: String,
    pub valid_from: String,
    pub valid_to: String,
    pub sans: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertProblem {
    Expired,
    NotYetValid,
    UnknownIssuer,
    SelfSigned,
    WrongHost,
}

impl CertProblem {
    /// The one-line the interstitial shows. No jargon, no lecture.
    pub fn message(self) -> &'static str {
        match self {
            CertProblem::Expired => "This certificate has expired.",
            CertProblem::NotYetValid => "This certificate is not valid yet.",
            CertProblem::UnknownIssuer => "The issuer of this certificate is not trusted.",
            CertProblem::SelfSigned => "This certificate was not issued by a certificate authority.",
            CertProblem::WrongHost => "This certificate belongs to a different site.",
        }
    }
}

/// Walk DER: one pass, one byte reader, the four tags we care about.
/// A real x509 parser this is not; a viewer-feeder it is.
pub fn summarize(der: &[u8]) -> Option<CertSummary> {
    let mut reader = Der { bytes: der, pos: 0 };
    // Certificate := SEQUENCE { tbsCertificate, signatureAlg, signature }
    let tbs = reader.sequence()?;
    let mut tbs_reader = Der { bytes: tbs, pos: 0 };

    // tbsCertificate := SEQUENCE { version [0] EXPLICIT optional, serial,
    // signature, issuer, validity, subject, subjectPublicKeyInfo, ... }
    if tbs_reader.peek() == Some(0xA0) {
        tbs_reader.skip_tagged()?;
    }
    let serial = tbs_reader.integer()?;
    tbs_reader.sequence()?; // signature algorithm
    let _issuer = tbs_reader.sequence()?;
    let validity = tbs_reader.sequence()?;
    let mut validity_reader = Der { bytes: validity, pos: 0 };
    let valid_from = validity_reader.utctime_or_generalized()?;
    let valid_to = validity_reader.utctime_or_generalized()?;
    let subject = tbs_reader.sequence()?;
    let subject_cn = extract_cn(subject)?;
    let issuer_cn = extract_cn(_issuer).unwrap_or_default();

    let mut summary = CertSummary {
        subject_cn,
        issuer_cn,
        serial_hex: serial.iter().map(|b| format!("{b:02X}")).collect(),
        valid_from,
        valid_to,
        sans: Vec::new(),
    };

    // SANs live in extensions [3] EXPLICIT: find them when present.
    if tbs_reader.peek() == Some(0xA3) {
        let extensions = tbs_reader.tagged(0xA3)?;
        let mut ext_reader = Der { bytes: extensions, pos: 0 };
        if let Some(list) = ext_reader.sequence() {
            summary.sans = extract_sans(list);
        }
    }
    Some(summary)
}

/// Validity against a wall clock, in the viewer's terms.
pub fn validity(summary: &CertSummary, now_unix: i64) -> Validity {
    let from = parse_utctime(&summary.valid_from).unwrap_or(i64::MIN);
    let to = parse_utctime(&summary.valid_to).unwrap_or(i64::MIN);
    if now_unix < from {
        Validity::Problem(CertProblem::NotYetValid)
    } else if now_unix > to {
        Validity::Problem(CertProblem::Expired)
    } else {
        Validity::Valid
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Validity {
    Valid,
    Problem(CertProblem),
}

/// Host check: exact SAN match or left-most wildcard, one level deep —
/// `*.example.com` covers `a.example.com`, never `a.b.example.com`.
pub fn host_matches(summary: &CertSummary, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    summary.sans.iter().any(|san| {
        let san = san.trim_start_matches('.').to_ascii_lowercase();
        if san == host {
            return true;
        }
        if let Some(prefix) = san.strip_prefix("*.") {
            return host
                .strip_suffix(&format!(".{prefix}"))
                .is_some_and(|label| !label.contains('.'));
        }
        false
    })
}

struct Der<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Der<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn read_tlv(&mut self) -> Option<(u8, &'a [u8])> {
        let tag = *self.bytes.get(self.pos)?;
        self.pos += 1;
        let first = *self.bytes.get(self.pos)?;
        self.pos += 1;
        let len = match first {
            0x00..=0x7F => first as usize,
            0x81 => {
                let len = *self.bytes.get(self.pos)? as usize;
                self.pos += 1;
                len
            }
            0x82 => {
                let hi = *self.bytes.get(self.pos)? as usize;
                let lo = *self.bytes.get(self.pos + 1)? as usize;
                self.pos += 2;
                (hi << 8) | lo
            }
            _ => return None,
        };
        let value = self.take(len)?;
        Some((tag, value))
    }

    fn sequence(&mut self) -> Option<&'a [u8]> {
        match self.read_tlv() {
            Some((0x30, value)) => Some(value),
            _ => None,
        }
    }

    fn integer(&mut self) -> Option<&'a [u8]> {
        match self.read_tlv() {
            Some((0x02, value)) => Some(value),
            _ => None,
        }
    }

    fn utctime_or_generalized(&mut self) -> Option<String> {
        match self.read_tlv() {
            Some((0x17 | 0x18, value)) => {
                Some(String::from_utf8_lossy(value).into_owned())
            }
            _ => None,
        }
    }

    fn skip_tagged(&mut self) -> Option<()> {
        self.read_tlv().map(|_| ())
    }

    fn tagged(&mut self, wanted: u8) -> Option<&'a [u8]> {
        match self.read_tlv() {
            Some((tag, value)) if tag == wanted => Some(value),
            _ => None,
        }
    }
}

fn extract_cn(name: &[u8]) -> Option<String> {
    let mut reader = Der { bytes: name, pos: 0 };
    let _rdn_sequence = reader.sequence()?;
    // RDNSequence := SEQUENCE OF RelativeDistinguishedName; each RDN is a
    // SET OF AttributeTypeAndValue (SEQUENCE { OID, value }).
    while reader.pos < name.len() {
        let mut set = Der {
            bytes: name,
            pos: reader.pos,
        };
        let Some(rdn) = set.sequence() else { break };
        reader.pos = set.pos;
        let mut attr = Der { bytes: rdn, pos: 0 };
        while attr.pos < rdn.len() {
            let mut pair = Der { bytes: rdn, pos: attr.pos };
            let Some(atv) = pair.sequence() else { break };
            attr.pos = pair.pos;
            let mut field = Der { bytes: atv, pos: 0 };
            if let Some((_tag, oid)) = field.read_tlv() {
                // 2.5.4.3 = commonName, DER-encoded as 06 03 55 04 03.
                if oid == [0x55, 0x04, 0x03] {
                    if let Some((0x0C | 0x13 | 0x16, value)) = field.read_tlv() {
                        return Some(String::from_utf8_lossy(value).into_owned());
                    }
                }
            }
        }
    }
    None
}

fn extract_sans(extensions: &[u8]) -> Vec<String> {
    let mut reader = Der { bytes: extensions, pos: 0 };
    let mut sans = Vec::new();
    // Extension ::= SEQUENCE { extnID OID, critical BOOL optional, value OCTET STRING }
    while reader.pos < extensions.len() {
        let mut cursor = Der { bytes: extensions, pos: reader.pos };
        let Some(ext) = cursor.sequence() else { break };
        reader.pos = cursor.pos;
        let mut field = Der { bytes: ext, pos: 0 };
        let Some((_tag, oid)) = field.read_tlv() else { continue };
        // 2.5.29.17 = subjectAltName, DER: 55 1D 11.
        if oid != [0x55, 0x1D, 0x11] {
            continue;
        }
        if field.peek() == Some(0x01) {
            field.read_tlv(); // critical flag
        }
        if let Some((0x04, inner)) = field.read_tlv() {
            let mut names = Der { bytes: inner, pos: 0 };
            if let Some(list) = names.sequence() {
                let mut entry = Der { bytes: list, pos: 0 };
                while entry.pos < list.len() {
                    if let Some((tag, value)) = entry.read_tlv() {
                        if tag == 0x82 {
                            sans.push(String::from_utf8_lossy(value).into_owned());
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }
    sans
}

/// UTCTime "YYMMDDHHMMSSZ" -> a unix timestamp, two-digit-window rule:
/// 00-49 => 20xx, 50-99 => 19xx.
fn parse_utctime(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 11 || bytes[10] != b'Z' {
        return None;
    }
    let two = |at: usize| -> Option<i64> { text.get(at..at + 2)?.parse().ok() };
    let year_raw = two(0)?;
    let year = if year_raw < 50 { 2000 + year_raw } else { 1900 + year_raw };
    let month = two(2)?;
    let day = two(4)?;
    let hour = two(6)?;
    let minute = two(8)?;
    // Seconds live at 10..12 when present; the Z then moves. Handle both.
    let second = if bytes.len() >= 13 && bytes[12] == b'Z' {
        two(10)?
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let (year, month) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month - 3) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
