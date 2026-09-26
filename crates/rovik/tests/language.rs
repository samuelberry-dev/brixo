//! End-to-end tests: run a script, check what it printed or which error it hit.

use rovik::{run, Interpreter, Trigger};

fn out(src: &str) -> Vec<String> {
    run(src).unwrap_or_else(|e| panic!("script failed: {e}\n---\n{src}"))
}

fn err(src: &str) -> String {
    match run(src) {
        Ok(output) => panic!("expected an error, but the script printed {output:?}"),
        Err(e) => e.to_string(),
    }
}

// --- basics ---

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(out("print(1 + 2 * 3, (1 + 2) * 3, 7 % 3, -2 * 3, 10 / 4)"), ["7 9 1 -6 2.5"]);
}

#[test]
fn whole_numbers_print_without_decimals() {
    assert_eq!(out("print(10, 2.5, 3.0)"), ["10 2.5 3"]);
}

#[test]
fn text_joins_with_plus() {
    assert_eq!(out(r#"print("Score: " + 5, 1 + 2, "a" + "b")"#), ["Score: 5 3 ab"]);
}

#[test]
fn comments_are_ignored() {
    let src = "-- line comment\n*** block\n comment ***\nprint(1) -- trailing\n";
    assert_eq!(out(src), ["1"]);
}

#[test]
fn compound_assignment() {
    assert_eq!(out("x = 10\nx += 5\nx -= 3\nx *= 2\nx /= 4\nprint(x)"), ["6"]);
}

// --- control flow ---

#[test]
fn if_elseif_else() {
    let src = r#"
fn size(n)
    if n > 10 then
        return "big"
    elseif n > 5 then
        return "medium"
    else
        return "small"
    end
end
print(size(20), size(7), size(1))
"#;
    assert_eq!(out(src), ["big medium small"]);
}

#[test]
fn one_line_if() {
    assert_eq!(out("if true then print(\"yes\") end"), ["yes"]);
}

#[test]
fn loops_break_and_continue() {
    let src = r#"
for i in 1..6 do
    if i == 2 then continue end
    if i == 5 then break end
    print(i)
end
"#;
    assert_eq!(out(src), ["1", "3", "4"]);
}

#[test]
fn while_loop() {
    assert_eq!(out("n = 0\nwhile n < 3 do\n n += 1\nend\nprint(n)"), ["3"]);
}

#[test]
fn for_over_list_map_and_text() {
    let src = r#"
for x in [1, 2] do print(x) end
for k in {b = 1, a = 2} do print(k) end
for c in "hi" do print(c) end
"#;
    assert_eq!(out(src), ["1", "2", "a", "b", "h", "i"]);
}

#[test]
fn empty_range_runs_zero_times() {
    assert_eq!(out("for i in 5..1 do print(i) end\nprint(\"done\")"), ["done"]);
}

// --- scope rules ---

#[test]
fn functions_update_existing_script_variables() {
    let src = "count = 0\nfn bump()\n count = count + 1\nend\nbump()\nbump()\nprint(count)";
    assert_eq!(out(src), ["2"]);
}

#[test]
fn new_names_inside_functions_stay_local() {
    let e = err("fn f()\n secret = 1\nend\nf()\nprint(secret)");
    assert!(e.contains("'secret' hasn't been given a value yet"), "{e}");
}

#[test]
fn if_blocks_do_not_create_scopes() {
    assert_eq!(out("if true then\n made_inside = 3\nend\nprint(made_inside)"), ["3"]);
}

#[test]
fn closures_remember_their_scope() {
    let src = r#"
fn counter()
    n = 0
    return fn ()
        n = n + 1
        return n
    end
end
c = counter()
c()
c()
print(c())
"#;
    assert_eq!(out(src), ["3"]);
}

#[test]
fn recursion_works() {
    let src = "fn fact(n)\n if n <= 1 then return 1 end\n return n * fact(n - 1)\nend\nprint(fact(10))";
    assert_eq!(out(src), ["3628800"]);
}

// --- lists and maps ---

#[test]
fn lists_start_at_one() {
    assert_eq!(out("l = [\"a\", \"b\", \"c\"]\nprint(l[1], l[3])"), ["a c"]);
}

#[test]
fn list_functions() {
    let src = "l = [1, 2]\npush(l, 3)\ninsert(l, 1, 0)\nprint(l)\nprint(pop(l), remove(l, 1), l, len(l))";
    assert_eq!(out(src), ["[0, 1, 2, 3]", "3 0 [1, 2] 2"]);
}

#[test]
fn maps_and_fields() {
    let src = "p = {name = \"Sam\", coins = 1}\np.coins += 4\np[\"level\"] = 2\nprint(p.coins, p.level, p.missing, keys(p))";
    assert_eq!(out(src), ["5 2 nil [\"coins\", \"level\", \"name\"]"]);
}

#[test]
fn lists_are_shared_not_copied() {
    assert_eq!(out("a = [1]\nb = a\npush(b, 2)\nprint(a)"), ["[1, 2]"]);
}

#[test]
fn equality_compares_contents() {
    assert_eq!(out("print([1, 2] == [1, 2], {a = 1} == {a = 1}, 1 == \"1\")"), ["true true false"]);
}

// --- builtins ---

#[test]
fn conversion_and_math_builtins() {
    let src = "print(num(\"42\") + 1, str(5) + \"!\", type(\"x\"), type([]), floor(2.7), round(2.5), abs(-3), min(4, 2, 9), max(4, 2, 9), sqrt(16))";
    assert_eq!(out(src), ["43 5! text list 2 3 3 2 9 4"]);
}

#[test]
fn random_stays_in_range() {
    let src = "for i in 1..200 do\n r = random(1, 3)\n if r < 1 or r > 3 or floor(r) != r then print(\"bad\") end\nend\nprint(\"ok\")";
    assert_eq!(out(src), ["ok"]);
}

#[test]
fn logic_operators() {
    assert_eq!(out("print(true and false, true or false, not true, nil or \"default\", not 1 == 2)"), ["false true false default true"]);
}

// --- events are recorded for the engine ---

#[test]
fn on_and_every_register_handlers() {
    let mut interp = Interpreter::new();
    interp
        .run_source("on touched(player)\n print(player)\nend\nevery 2 seconds\n print(1)\nend")
        .unwrap();
    let handlers = interp.handlers();
    assert_eq!(handlers.len(), 2);
    assert!(matches!(&handlers[0].trigger, Trigger::Event(e) if e == "touched"));
    assert!(matches!(handlers[1].trigger, Trigger::Every(s) if s == 2.0));
    assert!(interp.output.is_empty(), "handlers shouldn't run yet");
}

#[test]
fn handlers_can_be_called_by_the_engine() {
    let mut interp = Interpreter::new();
    interp
        .run_source("hits = 0\non touched(who)\n hits += 1\n print(who + \" hit\")\nend")
        .unwrap();
    let handler = interp.handlers()[0].function.clone();
    interp.call(handler.clone(), vec![rovik::Value::str("Sam")], 0).unwrap();
    interp.call(handler, vec![rovik::Value::str("Asher")], 0).unwrap();
    assert_eq!(interp.output, ["Sam hit", "Asher hit"]);
    assert_eq!(interp.global("hits").unwrap().display(), "2");
}

// --- beginner-friendly errors ---

#[test]
fn typo_suggests_the_right_name() {
    assert!(err("score = 1\nprint(scroe)").contains("Did you mean 'score'?"));
}

#[test]
fn missing_end_points_at_the_opener() {
    assert_eq!(err("x = 1\nwhile x < 3 do\n x += 1\n"), "line 2: this 'while' is missing its 'end'");
}

#[test]
fn single_equals_in_condition() {
    assert!(err("x = 1\nif x = 1 then end").contains("use '=='"));
}

#[test]
fn zero_index_explains_one_based_lists() {
    assert!(err("l = [1]\nprint(l[0])").contains("Positions start at 1"));
}

#[test]
fn nil_math_hints_at_unset_values() {
    assert!(err("p = {}\nprint(p.hp - 1)").contains("One side is nil"));
}

#[test]
fn useless_expression_is_flagged() {
    assert!(err("x = 1\nx + 1").contains("doesn't do anything with it"));
}

#[test]
fn wrong_argument_count() {
    assert_eq!(err("fn add(a, b)\n return a + b\nend\nadd(1)"), "line 4: add needs 2 values but got 1");
}

#[test]
fn infinite_loop_is_stopped() {
    assert!(err("while true do\nend").contains("ran for too long"));
}

#[test]
fn runaway_recursion_is_stopped() {
    assert!(err("fn f()\n f()\nend\nf()").contains("calling itself forever"));
}

#[test]
fn errors_report_the_right_line() {
    assert!(err("a = 1\nb = 2\n\n\nc = a / 0").starts_with("line 5:"));
}

#[test]
fn forks_draw_different_random_numbers() {
    // Every event handler runs on a fork; they must not all get the same
    // "random" sequence.
    let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut base = rovik::Interpreter::new();
    let sink = out.clone();
    base.on_print = Some(std::sync::Arc::new(move |t: &str| sink.lock().unwrap().push(t.to_string())));
    for _ in 0..8 {
        let mut fork = base.fork();
        fork.run_source("print(random(1, 1000000))").unwrap();
    }
    let seen: std::collections::HashSet<String> = out.lock().unwrap().iter().cloned().collect();
    assert!(seen.len() >= 7, "forks repeated numbers: {seen:?}");
}
