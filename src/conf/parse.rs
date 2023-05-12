use std::borrow::Cow;

use nom::{character::complete::{alphanumeric1, space1, space0, not_line_ending, self}, IResult, branch::alt, bytes::complete::{tag, is_not}, sequence::{tuple, delimited, preceded, pair, terminated, separated_pair}, combinator::{map, value, recognize, opt, map_parser, all_consuming}, multi::{separated_list1, many0, many0_count, many1_count, fold_many1, many1}};

use nom::character::complete::char as chara;

type Action = super::Action<String>;

fn keyname(input: &str) -> IResult<&str, &str> {
    recognize(many1_count(
        alt((alphanumeric1, tag("_") ))
    ))(input)
}

fn seq_sep(input: &str) -> IResult<&str, &str> {
    alt((
        delimited(
            space0,
            alt(
                (tag("↘"), tag("↗"))
            ),
            space0,
        ),
        space1))(input)
}

fn keyname_seq(input: &str) -> IResult<&str, Vec<&str>> {
    // not empty seq
    many1(terminated(keyname, opt(seq_sep)))(input)
}

/// doesn't accept empty string
fn not_escape(input: &str) -> IResult<&str, &str> {
    is_not(&['"', '\\', '\n'][..])(input)
}

fn escaped_char(input: &str) -> IResult<&str, char> {
    preceded(
        chara('\\'),
        alt((
            value('\n', chara('n')),
            value('\r', chara('r')),
            value('\t', chara('t')),
            value('\u{08}', chara('b')),
            value('\u{0C}', chara('f')),
            value('\\', chara('\\')),
            value('/', chara('/')),
            value('"', chara('"')),
        )),
    )(input)
}

fn lit_or_esc(input: &str) -> IResult<&str, Cow<str>> {
    alt((
        map(not_escape, Cow::from),
        map(escaped_char, |c| {
            Cow::from(c.to_string())
        }),
    ))(input)
}

fn content_with_escape(input: &str) -> IResult<&str, Cow<str>> {
    let (i, mut st) = lit_or_esc(input)?;
    let (i, vs) = many0(lit_or_esc)(i)?;
    if !vs.is_empty() {
        let st = st.to_mut();
        for s in vs.into_iter() {
            st.push_str(&s);
        }
    }
    Ok((i, st))
}

fn quoted(input: &str) -> IResult<&str, Cow<str>> {
    delimited(tag("\""),
              map(opt(content_with_escape), Option::unwrap_or_default),
              tag("\""))(input)
}
/// space-separated word or string in quotes
fn quoted_or_plain(input: &str) -> IResult<&str, Cow<str>> {
    alt((quoted,
         map(is_not(&['"', ',', ' ', '\t', '\n'][..]), Cow::from),
     ))(input)
}

fn varg(input: &str) -> IResult<&str, Vec<Cow<str>>> {
    separated_list1(space1, quoted_or_plain)(input)
}

fn arg1(input: &str) -> IResult<&str, &str> {
    alt((recognize(quoted),
         is_not(&['"', ',', '\n', '#', '\r'][..]),
    )) (input)
}

/// get string until there's a comma that's not in quotes
/// trim surrounding space
fn args_str_before_comma(input: &str) -> IResult<&str, &str> {
    let (input, _) = space0(input)?;
    let (input, s) = recognize(many0_count(arg1))(input)?;
    Ok((input, s.trim_end()))
}

fn action_text_arg(input: &str) -> IResult<&str, Cow<str>> {
    preceded(tag("text"),
             map_parser(args_str_before_comma,
                        alt((quoted,
                             map(not_line_ending, Cow::from)
                        )))
    )(input)
}

fn key_combination(input: &str) -> IResult<&str, Vec<&str>> {
    separated_list1(tuple((space0, tag("+"), space0)), keyname)(input)
}

fn key_combi_list(input: &str) -> IResult<&str, Vec<Vec<&str>>> {
    separated_list1(space1, key_combination)(input)
}

fn action_key_combi_multi(input: &str) -> IResult<&str, Vec<Action>> {
    preceded(pair(tag("key"), space1),
             map(key_combi_list, |vvs| {
                 vvs.into_iter().map(|kc| {
                     Action::KeyStroke(
                         kc.into_iter().map(|s| s.to_string()).collect())
                 }).collect()
             }))(input)
}

fn action_mouse_click(input: &str) -> IResult<&str, Action> {
    preceded(tag("mouse"),
             map(
                 map_parser(args_str_before_comma, complete::u8),
                 Action::MouseClick)
    )(input)
}


fn action_repeat(input: &str) -> IResult<&str, Action> {
    preceded(tag("repeat"),
             map(
                 map_parser(args_str_before_comma, complete::u8),
                 Action::Repeat)
    )(input)
}

fn action_exec(input: &str) -> IResult<&str, Action> {
    preceded(pair(tag("exec"), space1),
             map(varg, |v| Action::Exec( v.into_iter().map(|s| s.to_string()).collect()))
    )(input)
}

fn actions_before_comma(input: &str) -> IResult<&str, Vec<Action>> {
    alt((
        map(action_text_arg, |s| {
            vec![Action::Text(s.to_string())]
        }),
        action_key_combi_multi,
        map(action_exec, |a| vec![a]),
        map(action_mouse_click, |a| vec![a]),
        map(action_repeat, |a| vec![a])
    ))(input)
}

fn actions_separated_by_comma(input: &str) -> IResult<&str, Vec<Action>> {
    fold_many1(
        terminated(actions_before_comma, pair(space0, opt(pair(tag(","), space0)))),
        Vec::new,
        |mut acc: Vec<_>, item| {
            acc.extend(item);
            acc
        }
    )(input)
}

fn assignment(input: &str) -> IResult<&str,  (Vec<&str>,Vec<Action>)> {
    separated_pair(keyname_seq, pair(tag("="), space0), actions_separated_by_comma)(input)
}

pub(crate) fn assignment_line(input: &str) -> IResult<&str, Option< (Vec<&str>,Vec<Action>)>> {
    all_consuming(
        delimited(space0,
                  opt(assignment),
                  opt(preceded(tag("#"), not_line_ending))
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nom1() {
        assert_eq!(keyname("21cZ%1"), Ok(("%1", "21cZ")));
        assert_eq!(keyname("N ↘ "), Ok((" ↘ ", "N")));
    }
     #[test]
    fn nom2() {
        assert_eq!(seq_sep(" ↘ "), Ok(("", "↘")));
        assert_eq!(seq_sep("↘ "), Ok(("", "↘")));
        assert_eq!(seq_sep("↘"), Ok(("", "↘")));
        assert_eq!(seq_sep(" ↗"), Ok(("", "↗")));
        assert_eq!(seq_sep(" "), Ok(("", " ")));
        assert_eq!(seq_sep("  "), Ok(("", "  ")));
        assert!(seq_sep("").is_err());
    }
    #[test]
    fn nom3() {
        assert_eq!(keyname_seq("N ↘ T ↘ T ↗ N ↗ "),
                   Ok(("", vec!["N", "T", "T", "N"])));
        assert_eq!(keyname_seq("N↘ T↘ T↗"),
                   Ok(("", vec!["N", "T", "T"])));
        assert_eq!(keyname_seq("N↘ T↘T↗ "),
                   Ok(("", vec!["N", "T", "T"])));
        assert_eq!(keyname_seq("M↘ 1↘ 1↗ M↗ "),
                   Ok(("", vec!["M", "1", "1", "M"])));
    }
    #[test]
    fn nom4() {
        assert_eq!(not_escape("abc"), Ok(("", "abc")));
        assert_eq!(not_escape("kl\\"), Ok(("\\", "kl")));
        let ee = not_escape("").unwrap_err();
        assert!(!ee.is_incomplete(), "{ee:?}");
    }
    #[test]
    fn nome() {
        assert_eq!(escaped_char(r#"\\"#), Ok(("", '\\')));
        assert_eq!(escaped_char("\\n"), Ok(("", '\n')));
        assert_eq!(escaped_char("\\\\b"), Ok(("b", '\\')));
        assert_eq!(escaped_char("\\nc"), Ok(("c", '\n')));
        let ee = escaped_char("").unwrap_err();
        assert!(!ee.is_incomplete(), "{ee:?}");
    }
    #[test]
    fn nom5() {
        let (i, s) = lit_or_esc("abc").unwrap();
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!((i, s.as_ref()), ("", "abc"));
        let (i, s) = lit_or_esc("abc\\").unwrap();
        assert_eq!((i, s.as_ref()), ("\\", "abc"));
        let (i, s) = lit_or_esc("\\babc\\").unwrap();
        assert_eq!((i, s.as_ref()), ("abc\\", "\u{8}"));
        let ee = lit_or_esc("").unwrap_err();
        assert!(!ee.is_incomplete());
    }
     #[test]
    fn nom6() {
        let (i, s) = many0(lit_or_esc)("").unwrap();
        assert_eq!(i, "");
        assert!(s.is_empty());
        let (i, s) = many0(lit_or_esc)("abc").unwrap();
        assert_eq!(i, "");
        assert_eq!(s.as_ref(),  vec!["abc"]);
        let (i, s) = content_with_escape("abc").unwrap();
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!((i, s.as_ref()), ("", "abc"));
        let (i, s) =  content_with_escape("abc\\").unwrap();
        assert_eq!((i, s.as_ref()), ("\\", "abc"));
        let (i, s) =  content_with_escape("\\babc\\").unwrap();
        assert!(matches!(s, Cow::Owned(_)));
        assert_eq!((i, s.as_ref()), ("\\", "\u{8}abc"));
    }

    #[test]
    fn nom7() {
        let (i, s) = quoted_or_plain("abc").unwrap();
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!((i, s.as_ref()), ("", "abc"));
        let (i, s) = quoted_or_plain("\"abc\"").unwrap();
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!((i, s.as_ref()), ("", "abc"));
        let (i, s) = quoted_or_plain("\"a\\nbc\"").unwrap();
        assert_eq!((i, s.as_ref()), ("", "a\nbc"));
    }
    #[test]
    fn nom8() {
        let (i, s) = varg("abc").unwrap();
        assert_eq!(i, "");
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0], Cow::Borrowed(_)));
        assert_eq!(s[0].as_ref(),  "abc");
        let (i, s) = varg("abc def ").unwrap();
        assert_eq!(i, " ");
        assert_eq!(s.len(), 2);
        assert!(matches!(s[1], Cow::Borrowed(_)));
        assert_eq!(s[0].as_ref(),  "abc");
        assert_eq!(s[1].as_ref(),  "def");
        let (i, s) = varg("abc \"de\\tf\", ").unwrap();
        assert_eq!(i, ", ");
        assert_eq!(s.len(), 2);
        assert!(matches!(s[0], Cow::Borrowed(_)));
        assert_eq!(s[0].as_ref(),  "abc");
        assert_eq!(s[1].as_ref(),  "de\tf");
    }
    #[test]
    fn nom9() {
        let (i, s) = arg1("abc d f").unwrap();
        assert_eq!((i, s), ("", "abc d f"));
        let (i, s) = arg1(r#"abc d f",","#).unwrap();
        assert_eq!((i, s), ("\",\",", "abc d f"));
        let (i, s) = arg1("dof\"").unwrap();
        assert_eq!((i, s), ("\"", "dof"));
    }
    #[test]
    fn nom10() {
        let (i, s) = args_str_before_comma(r#"abc d f",","#).unwrap();
        assert_eq!((i, s), (",", "abc d f\",\""));
        let (i, s) = args_str_before_comma("abc d f \",\" ,").unwrap();
        assert_eq!((i, s), (",", "abc d f \",\""));
    }
    #[test]
    fn nom11() {
        let (i, s) = action_text_arg("text abc d f ,").unwrap();
        assert_eq!((i, s.as_ref()), (",", "abc d f"));
    }
    #[test]
    fn nom12() {
        let (i, s) = key_combination("ctrl+ c ").unwrap();
        assert_eq!((i, s), (" ", vec!["ctrl", "c"]));
    }
    #[test]
    fn nom13() {
        let (i, s) = key_combi_list("ctrl+ c  ctrl+s").unwrap();
        assert_eq!((i, s), ("", vec![vec!["ctrl", "c"], vec!["ctrl", "s"]]));
    }
    #[test]
    fn nom14() {
        let (i, s) = actions_separated_by_comma("text 未来, key ctrl+ c ctrl+x #").unwrap();
        assert_eq!((i, s), ("#", vec![
            Action::Text("未来".to_string()),
            Action::KeyStroke(vec!["ctrl".to_string(), "c".to_string()]),
            Action::KeyStroke(vec!["ctrl".to_string(), "x".to_string()]),
        ]));
    }
     #[test]
    fn nom15() {
        let (i, s) = action_exec("exec abc d f ,").unwrap();
        assert_eq!((i, s), (" ,",Action::Exec(vec!("abc".to_string(), "d".to_string(), "f".to_string()))));
    }
    #[test]
    fn nom16() {
        let (i, s) = actions_separated_by_comma("text 未来, key ctrl+ c ctrl+x #").unwrap();
        assert_eq!((i, s), ("#", vec![
            Action::Text("未来".to_string()),
            Action::KeyStroke(vec!["ctrl".to_string(), "c".to_string()]),
            Action::KeyStroke(vec!["ctrl".to_string(), "x".to_string()]),
        ]));
    }
    #[test]
    fn noml() {}
}
