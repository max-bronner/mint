use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::error::{EvalErrorKind, HostError, ParseErrorKind};
use crate::eval::{Budget, Host};
use crate::limits;
use crate::value::{Env, Value};
use crate::{Chunk, Script};

#[derive(Default)]
struct Recorder {
    seen: Vec<String>,
}

impl Host for Recorder {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError> {
        let mut line = String::from(name);
        line.push('(');
        for (index, arg) in args.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            match arg {
                Value::Str(text) => line.push_str(text),
                other => {
                    let _ = core::fmt::Write::write_fmt(&mut line, format_args!("{other}"));
                }
            }
        }
        line.push(')');
        self.seen.push(line);
        Ok(match name {
            "one" => Value::Number(1.0),
            "text" => Value::string("abc"),
            "yes" => Value::Bool(true),
            "no" => Value::Bool(false),
            "nothing" => Value::Nil,
            _ => Value::string(name),
        })
    }
}

struct Refusing;

impl Host for Refusing {
    fn call(&mut self, name: &str, _args: &[Value]) -> Result<Value, HostError> {
        let mut message = String::from("unknown call ");
        message.push_str(name);
        Err(HostError::new(message))
    }
}

fn run(source: &str) -> (Value, Vec<String>) {
    let script = Script::parse(source).expect("parses");
    let mut host = Recorder::default();
    let mut env = Env::new();
    let mut budget = Budget::default();
    let value = script
        .run(&mut env, &mut host, &mut budget)
        .expect("evaluates");
    (value, host.seen)
}

fn parse_error(source: &str) -> ParseErrorKind {
    Script::parse(source).expect_err("is refused").kind
}

fn eval_error(source: &str) -> EvalErrorKind {
    let script = Script::parse(source).expect("parses");
    let mut host = Recorder::default();
    script
        .run(&mut Env::new(), &mut host, &mut Budget::default())
        .expect_err("fails")
        .kind
}

#[test]
fn every_statement_form_of_the_subset_runs() {
    let cases: [(&str, Value); 7] = [
        ("return \"clip\"", Value::string("clip")),
        ("Show(\"a\")", Value::Nil),
        ("x = 2 return x", Value::Number(2.0)),
        ("local x = 3 return x", Value::Number(3.0)),
        ("if true then return \"t\" end", Value::string("t")),
        ("if false then return \"t\" end", Value::Nil),
        ("return", Value::Nil),
    ];
    for (source, expected) in cases {
        assert_eq!(run(source).0, expected, "{source}");
    }
}

#[test]
fn every_operator_of_the_subset_evaluates() {
    let cases: [(&str, Value); 16] = [
        ("return 1 == 1", Value::Bool(true)),
        ("return 2 < 2", Value::Bool(false)),
        ("return \"a\" < \"b\"", Value::Bool(true)),
        ("return \"a\" == \"a\"", Value::Bool(true)),
        ("return \"a\" == 1", Value::Bool(false)),
        ("return \"a\" ~= \"b\"", Value::Bool(true)),
        ("return 1 < 2", Value::Bool(true)),
        ("return 2 <= 2", Value::Bool(true)),
        ("return 3 > 2", Value::Bool(true)),
        ("return 3 >= 4", Value::Bool(false)),
        ("return \"a\" .. \"b\"", Value::string("ab")),
        ("return 1 + 2", Value::Number(3.0)),
        ("return 1 - 2", Value::Number(-1.0)),
        ("return 2 * 3", Value::Number(6.0)),
        ("return 3 / 2", Value::Number(1.5)),
        ("return not nil", Value::Bool(true)),
    ];
    for (source, expected) in cases {
        assert_eq!(run(source).0, expected, "{source}");
    }
}

#[test]
fn unary_minus_applies_to_a_decimal_literal() {
    assert_eq!(run("return -0.5 < 0").0, Value::Bool(true));
}

#[test]
fn and_binds_tighter_than_or() {
    assert_eq!(
        run("return false and false or true").0,
        Value::Bool(true),
        "(false and false) or true"
    );
    assert_eq!(run("return true or false and false").0, Value::Bool(true));
}

#[test]
fn comparison_binds_tighter_than_and() {
    assert_eq!(run("return 1 == 1 and 2 == 2").0, Value::Bool(true));
}

#[test]
fn concatenation_binds_tighter_than_comparison() {
    assert_eq!(run("return \"a\" .. \"b\" == \"ab\"").0, Value::Bool(true));
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(run("return 1 + 2 * 3").0, Value::Number(7.0));
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(run("return (1 + 2) * 3").0, Value::Number(9.0));
}

#[test]
fn a_false_left_side_of_and_skips_the_right() {
    assert_eq!(run("if no() and one() then end").1, vec!["no()"]);
}

#[test]
fn a_true_left_side_of_or_skips_the_right() {
    assert_eq!(run("if yes() or one() then end").1, vec!["yes()"]);
}

#[test]
fn and_or_yield_the_operand_not_a_boolean() {
    assert_eq!(
        run("return nil or \"fallback\"").0,
        Value::string("fallback")
    );
    assert_eq!(run("return \"a\" and \"b\"").0, Value::string("b"));
}

#[test]
fn an_elseif_chain_takes_the_first_matching_arm_only() {
    let source = "
        x = 2
        if x == 1 then return \"one\"
        elseif x == 2 then return \"two\"
        elseif x == 2 then return \"again\"
        else return \"other\" end";
    assert_eq!(run(source).0, Value::string("two"));
}

#[test]
fn an_elseif_chain_falls_through_to_else() {
    let source = "
        x = 9
        if x == 1 then return \"one\"
        elseif x == 2 then return \"two\"
        else return \"other\" end";
    assert_eq!(run(source).0, Value::string("other"));
}

#[test]
fn arguments_are_evaluated_left_to_right_and_inner_calls_first() {
    let (_, seen) = run("outer(text(), inner(one()))");
    assert_eq!(
        seen,
        vec!["text()", "one()", "inner(1)", "outer(abc,inner)"]
    );
}

#[test]
fn a_returned_value_of_a_call_reaches_the_next_statement() {
    let (value, seen) = run("Prev = text() if Prev == \"abc\" then return Prev end");
    assert_eq!(value, Value::string("abc"));
    assert_eq!(seen, vec!["text()"]);
}

#[test]
fn a_caller_pre_set_variable_is_readable() {
    let script = Script::parse("if Prev == \"Idle\" then return \"go\" end").expect("parses");
    let mut env = Env::new();
    env.set("Prev", Value::string("Idle")).expect("fits");
    let value = script
        .run(&mut env, &mut Recorder::default(), &mut Budget::default())
        .expect("evaluates");
    assert_eq!(value, Value::string("go"));
}

#[test]
fn a_host_refusal_is_propagated_with_the_call_name() {
    let script = Script::parse("Missing()").expect("parses");
    let error = script
        .run(&mut Env::new(), &mut Refusing, &mut Budget::default())
        .expect_err("fails");
    match error.kind {
        EvalErrorKind::Host { name, error } => {
            assert_eq!(name, "Missing");
            assert_eq!(error.message, "unknown call Missing");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_semicolon_is_a_comment_in_a_block_and_a_statement_end_in_a_file() {
    assert_eq!(run("; Show(\"a\")\nreturn \"x\"").1, Vec::<String>::new());
    let chunk = Chunk::parse("function f() Show(\"a\"); end").expect("parses");
    let mut host = Recorder::default();
    chunk
        .call("f", &[], &mut host, &mut Budget::default())
        .expect("evaluates");
    assert_eq!(host.seen, vec!["Show(a)"]);
}

#[test]
fn both_host_comment_markers_and_lua_comments_are_ignored_in_a_block() {
    let (value, seen) = run("// Show(\"a\")\n-- Show(\"b\")\n; Show(\"c\")\nreturn \"x\"");
    assert_eq!(value, Value::string("x"));
    assert_eq!(seen, Vec::<String>::new());
}

#[test]
fn a_chunk_function_is_called_by_name_with_arguments() {
    let chunk = Chunk::parse(
        "-- a file of handlers\nfunction handle(self, data)\n  Act(self, data)\n  return data\nend",
    )
    .expect("parses");
    let mut host = Recorder::default();
    let value = chunk
        .call(
            "handle",
            &[Value::string("unit"), Value::string("payload")],
            &mut host,
            &mut Budget::default(),
        )
        .expect("evaluates");
    assert_eq!(value, Value::string("payload"));
    assert_eq!(host.seen, vec!["Act(unit,payload)"]);
    assert_eq!(
        chunk.function("handle").map(crate::Function::arity),
        Some(2)
    );
}

#[test]
fn a_chunk_function_calls_another_before_the_host() {
    let chunk = Chunk::parse("function a() return b() end function b() return \"inner\" end")
        .expect("parses");
    let mut host = Recorder::default();
    let value = chunk
        .call("a", &[], &mut host, &mut Budget::default())
        .expect("evaluates");
    assert_eq!(value, Value::string("inner"));
    assert_eq!(host.seen, Vec::<String>::new());
}

#[test]
fn a_chunk_function_called_with_the_wrong_arity_is_refused() {
    let chunk = Chunk::parse("function f(a) return a end").expect("parses");
    let error = chunk
        .call("f", &[], &mut Recorder::default(), &mut Budget::default())
        .expect_err("fails");
    assert!(matches!(
        error.kind,
        EvalErrorKind::WrongArity {
            expected: 1,
            found: 0,
            ..
        }
    ));
}

#[test]
fn constructs_outside_the_subset_are_refused_by_name() {
    let cases: [(&str, &str); 12] = [
        ("while true do end", "a `while` loop"),
        ("for i = 1, 2 do end", "a `for` loop"),
        ("repeat until true", "a `repeat` loop"),
        ("do end", "a `do` block"),
        ("x = {1, 2}", "a table constructor"),
        ("x = t[1]", "an index expression `[]`"),
        ("x = t.field", "field access `.`"),
        ("obj:method()", "a method call `:`"),
        ("x = 1 % 2", "the modulo operator `%`"),
        ("x = 2 ^ 3", "the exponent operator `^`"),
        ("x = #s", "the length operator `#`"),
        ("local function f() end", "a local function"),
    ];
    for (source, expected) in cases {
        match parse_error(source) {
            ParseErrorKind::Unsupported(what) => assert_eq!(what, expected, "{source}"),
            other => panic!("{source}: {other:?}"),
        }
    }
}

#[test]
fn a_function_declaration_is_refused_inside_a_block_and_required_in_a_file() {
    assert_eq!(
        parse_error("function f() end"),
        ParseErrorKind::Unsupported("a nested function declaration")
    );
    assert!(matches!(
        Chunk::parse("x = 1").expect_err("is refused").kind,
        ParseErrorKind::Expected { .. }
    ));
}

#[test]
fn a_parse_error_names_the_line_it_is_on() {
    let error = Script::parse("x = 1\ny = 2\nwhile true do end").expect_err("is refused");
    assert_eq!(error.line, 3);
}

#[test]
fn an_unbalanced_block_is_refused_rather_than_truncated() {
    assert!(matches!(
        parse_error("if true then return \"a\""),
        ParseErrorKind::UnexpectedEnd(_)
    ));
}

#[test]
fn a_return_before_the_end_of_a_block_is_refused() {
    assert!(matches!(
        parse_error("return \"a\" Show(\"b\")"),
        ParseErrorKind::Expected { .. }
    ));
}

#[test]
fn a_malformed_literal_is_refused() {
    assert!(matches!(
        parse_error("x = \"unterminated"),
        ParseErrorKind::UnterminatedString
    ));
    assert!(matches!(
        parse_error("x = \"\\q\""),
        ParseErrorKind::UnknownEscape(_)
    ));
    assert!(matches!(
        parse_error("x = 1.2.3"),
        ParseErrorKind::MalformedNumber(_)
    ));
}

#[test]
fn a_chunk_declaring_one_name_twice_is_refused() {
    assert!(matches!(
        Chunk::parse("function f() end function f() end")
            .expect_err("is refused")
            .kind,
        ParseErrorKind::DuplicateFunction(_)
    ));
}

#[test]
fn a_source_over_the_byte_cap_is_refused() {
    let mut source = String::new();
    while source.len() < 1024 * 1024 {
        source.push_str("x = 1\n");
    }
    assert!(matches!(
        parse_error(&source),
        ParseErrorKind::SourceTooLong { .. }
    ));
}

#[test]
fn a_source_over_the_token_cap_is_refused() {
    let mut source = String::new();
    while source.len() < limits::MAX_SOURCE_BYTES.saturating_sub(8) {
        source.push_str("x = 1\n");
    }
    assert!(matches!(
        parse_error(&source),
        ParseErrorKind::TooManyTokens { .. }
    ));
}

#[test]
fn nesting_past_the_depth_cap_is_refused_rather_than_overflowing_the_stack() {
    let mut source = String::new();
    for _ in 0..(limits::MAX_DEPTH + 8) {
        source.push_str("if true then ");
    }
    source.push_str("x = 1 ");
    for _ in 0..(limits::MAX_DEPTH + 8) {
        source.push_str("end ");
    }
    assert!(matches!(
        parse_error(&source),
        ParseErrorKind::TooDeep { .. }
    ));

    let mut nested = String::from("x = ");
    for _ in 0..(limits::MAX_DEPTH + 8) {
        nested.push('(');
    }
    nested.push('1');
    assert!(matches!(
        parse_error(&nested),
        ParseErrorKind::TooDeep { .. }
    ));

    for link in ["1 or ", "1 and ", "1 == ", "1 + ", "1 * "] {
        let chain = format!("x = {}1", link.repeat(8000));
        assert!(
            matches!(parse_error(&chain), ParseErrorKind::TooDeep { .. }),
            "{link}"
        );
    }
    let longest = format!("x = {}1", "1 + ".repeat(limits::MAX_DEPTH as usize - 1));
    assert!(Script::parse(&longest).is_ok());
}

#[test]
fn a_call_over_the_argument_cap_is_refused() {
    let mut source = String::from("f(1");
    for _ in 0..limits::MAX_ARGS {
        source.push_str(", 1");
    }
    source.push(')');
    assert!(matches!(
        parse_error(&source),
        ParseErrorKind::TooManyArguments { .. }
    ));
}

#[test]
fn recursion_between_two_functions_exhausts_the_budget_rather_than_running_on() {
    let chunk =
        Chunk::parse("function a() return b() end function b() return a() end").expect("parses");
    let mut budget = Budget::new(20);
    let error = chunk
        .call("a", &[], &mut Recorder::default(), &mut budget)
        .expect_err("fails");
    assert_eq!(error.kind, EvalErrorKind::BudgetExhausted);
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn recursion_past_the_call_depth_cap_is_refused() {
    let chunk =
        Chunk::parse("function a() return b() end function b() return a() end").expect("parses");
    let error = chunk
        .call(
            "a",
            &[],
            &mut Recorder::default(),
            &mut Budget::new(1_000_000),
        )
        .expect_err("fails");
    assert!(matches!(error.kind, EvalErrorKind::CallTooDeep { .. }));
}

#[test]
fn a_concatenation_over_the_string_cap_is_refused() {
    let mut source = String::from("x = \"");
    for _ in 0..(limits::MAX_STRING_BYTES / 2) {
        source.push('a');
    }
    source.push_str("\" return x .. x .. x");
    assert!(matches!(
        eval_error(&source),
        EvalErrorKind::StringTooLong { .. }
    ));
}

#[test]
fn more_variables_than_the_cap_are_refused() {
    let mut source = String::new();
    for index in 0..(limits::MAX_VARIABLES + 1) {
        let _ = core::fmt::Write::write_fmt(&mut source, format_args!("v{index} = 1\n"));
    }
    assert!(matches!(
        eval_error(&source),
        EvalErrorKind::TooManyVariables { .. }
    ));
}

#[test]
fn reading_a_name_that_was_never_assigned_is_an_error_not_nil() {
    match eval_error("if Prve == \"x\" then end") {
        EvalErrorKind::UnknownVariable(name) => assert_eq!(name, "Prve"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ordering_two_types_lua_does_not_order_is_an_error() {
    assert!(matches!(
        eval_error("return \"a\" < 1"),
        EvalErrorKind::IncomparableTypes { .. }
    ));
}

#[test]
fn concatenating_a_non_string_is_refused_by_type() {
    assert!(matches!(
        eval_error("return \"a\" .. 1"),
        EvalErrorKind::TypeMismatch { op: "..", .. }
    ));
}

#[test]
fn arithmetic_on_a_string_is_refused_by_type() {
    assert!(matches!(
        eval_error("return \"a\" + 1"),
        EvalErrorKind::TypeMismatch { op: "+", .. }
    ));
}

#[test]
fn two_runs_of_one_script_consume_the_same_budget_and_give_the_same_result() {
    let source = "
        Prev = text()
        if Prev == \"abc\" and one() < 2 then
            Show(Prev)
            return Prev .. \"!\"
        else
            return \"other\"
        end";
    let script = Script::parse(source).expect("parses");
    let mut results = Vec::new();
    for _ in 0..2 {
        let mut host = Recorder::default();
        let mut budget = Budget::new(limits::DEFAULT_BUDGET);
        let value = script
            .run(&mut Env::new(), &mut host, &mut budget)
            .expect("evaluates");
        results.push((value, host.seen, budget.remaining()));
    }
    let first = results.first().expect("two runs");
    let second = results.get(1).expect("two runs");
    assert_eq!(first, second);
    assert_eq!(first.0, Value::string("abc!"));
}

#[test]
fn an_environment_iterates_in_name_order_whatever_the_assignment_order() {
    let mut forwards = Env::new();
    let mut backwards = Env::new();
    for name in ["zeta", "alpha", "mid"] {
        forwards.set(name, Value::string(name)).expect("fits");
    }
    for name in ["mid", "alpha", "zeta"] {
        backwards.set(name, Value::string(name)).expect("fits");
    }
    let names: Vec<&str> = forwards.iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["alpha", "mid", "zeta"]);
    assert_eq!(forwards, backwards);
}

#[test]
fn call_names_lists_every_call_including_nested_ones() {
    let script = Script::parse("if a() then b(c()) end").expect("parses");
    assert_eq!(script.call_names(), ["a", "b", "c"]);
}

#[test]
fn a_long_comment_is_refused_by_name_rather_than_read_as_one_line() {
    assert_eq!(
        parse_error("--[[ two\nlines ]] x = 1"),
        ParseErrorKind::Unsupported("a long comment `--[[ ]]`")
    );
}

#[test]
fn a_hexadecimal_number_is_refused_by_name() {
    assert_eq!(
        parse_error("x = 0x10"),
        ParseErrorKind::Unsupported("a hexadecimal number")
    );
}

#[test]
fn an_error_message_names_the_line_and_the_cause() {
    let error = Script::parse("x = 1\ny = {}").expect_err("is refused");
    let mut text = String::new();
    let _ = core::fmt::Write::write_fmt(&mut text, format_args!("{error}"));
    assert_eq!(
        text,
        "line 2: a table constructor is outside the supported subset"
    );
}

#[test]
fn tostring_is_answered_without_the_host() {
    let (value, seen) = run("return tostring(\"show_rock\") .. tostring(nil) .. tostring(true)");
    assert_eq!(value, Value::string("show_rockniltrue"));
    assert!(seen.is_empty());
}

#[test]
fn tonumber_converts_a_numeric_string_and_passes_a_number_through() {
    let (value, seen) = run("return tonumber(\"3.5\")");
    assert_eq!(value, Value::Number(3.5));
    assert!(seen.is_empty());

    let (value, _) = run("return tonumber(2)");
    assert_eq!(value, Value::Number(2.0));

    let (value, _) = run("return tonumber(\"not a number\")");
    assert_eq!(value, Value::Nil);
}
