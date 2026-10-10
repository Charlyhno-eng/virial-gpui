1|use super::*;
2|use crate::infrastructure::editor_io::{decode, DocumentFormat, LineEnding};
3|
4|fn format() -> DocumentFormat {
5|    DocumentFormat {
6|        line_ending: LineEnding::Lf,
7|        bom: false,
8|        latin1_fallback: false,
9|    }
10|}
11|
12|fn make(text: &str) -> Editor {
13|    Editor::new(text, format())
14|}
15|
16|#[test]
17|fn round_trips_exactly_and_keeps_the_trailing_newline() {
18|    for text in ["", "\n", "a\n", "a\nb", "a\n\nb\n", "\n\n\n"] {
19|        assert_eq!(make(text).text(), text, "round trip {text:?}");
20|    }
21|}
22|
23|#[test]
24|fn normalizes_crlf_and_keeps_the_original_ending_for_saving() {
25|    let editor = make("a\r\nb\r\n");
26|    assert_eq!(editor.text(), "a\nb\n");
27|    assert_eq!(editor.line_ending(), LineEnding::LfCr);
28|}
29|
30|#[test]
31|fn typing_inserts_at_the_cursor_and_marks_the_document_dirty() {
32|    let mut editor = make("hello\n");
33|    assert!(!editor.is_dirty());
34|    editor.set_cursor(5, false);
35|    editor.insert(" world");
36|    assert_eq!(editor.text(), "hello world\n");
37|    assert!(editor.is_dirty());
38|    assert_eq!(editor.cursor(), 11);
39|}
40|
41|#[test]
42|fn typing_over_a_selection_replaces_it() {
43|    let mut editor = make("hello world\n");
44|    editor.set_cursor(0, false);
45|    editor.set_cursor(5, true);
46|    assert_eq!(editor.text_in(editor.selection()), "hello");
47|    editor.insert("bye");
48|    assert_eq!(editor.text(), "bye world\n");
49|}
50|
51|#[test]
52|fn delete_removes_a_grapheme_not_a_byte() {
53|    let mut editor = make("a👋b\n");
54|    editor.set_cursor(3, false);
55|    editor.delete(false);
56|    assert_eq!(editor.text(), "a👋\n");
57|    editor.delete(true);
58|    assert_eq!(editor.text(), "a\n");
59|}
60|
61|#[test]
62|fn delete_merges_lines_at_the_end_and_start() {
63|    let mut editor = make("ab\ncd\n");
64|    editor.set_cursor(2, false);
65|    editor.delete(false);
66|    assert_eq!(editor.text(), "abcd\n");
67|    editor.set_cursor(2, false);
68|    editor.delete(true);
69|    assert_eq!(editor.text(), "abcd\n");
70|    // Backspace at the very start of the buffer is a no-op, not a panic.
71|    editor.set_cursor(0, false);
72|    editor.delete(true);
73|    assert_eq!(editor.text(), "abcd\n");
74|}
75|
76|#[test]
77|fn newline_keeps_the_indentation_and_indents_after_a_brace() {
78|    let mut editor = make("    let x = 1;\n");
79|    editor.set_cursor(14, false);
80|    editor.newline(LineEnding::Lf);
81|    assert_eq!(editor.text(), "    let x = 1;\n    \n");
82|
83|    let mut braces = make("fn main() {\n");
84|    braces.set_cursor(11, false);
85|    braces.newline(LineEnding::Lf);
86|    assert_eq!(braces.text(), "fn main() {\n    \n");
87|}
88|
89|#[test]
90|fn undo_groups_a_run_of_typing_and_redo_restores_it() {
91|    let mut editor = make("\n");
92|    editor.set_cursor(0, false);
93|    for ch in "abc".chars() {
94|        editor.insert(&ch.to_string());
95|    }
96|    assert_eq!(editor.text(), "abc\n");
97|    editor.undo();
98|    assert_eq!(editor.text(), "\n", "one Ctrl+Z removes the run");
99|    editor.redo();
100|    assert_eq!(editor.text(), "abc\n");
101|}
102|
103|#[test]
104|fn undo_restores_a_deleted_word_and_the_cursor() {
105|    let mut editor = make("hello world\n");
106|    editor.set_cursor(11, false);
107|    editor.set_cursor(6, true);
108|    editor.insert("");
109|    assert_eq!(editor.text(), "hello \n");
110|    editor.undo();
111|    assert_eq!(editor.text(), "hello world\n");
112|    assert!(editor.can_redo());
113|    editor.redo();
114|    assert_eq!(editor.text(), "hello \n");
115|}
116|
117|#[test]
118|fn typing_after_an_undo_discards_the_redo_stack() {
119|    let mut editor = make("\n");
120|    editor.set_cursor(0, false);
121|    editor.insert("a");
122|    editor.undo();
123|    assert!(editor.can_redo());
124|    editor.insert("b");
125|    assert!(!editor.can_redo());
126|    assert_eq!(editor.text(), "b\n");
127|}
128|
129|#[test]
130|fn shift_indents_and_outdents_every_line_of_the_selection() {
131|    let mut editor = make("a\nb\nc\n");
132|    editor.set_cursor(0, false);
133|    editor.set_cursor(3, true);
134|    editor.shift(false);
135|    assert_eq!(editor.text(), "    a\n    b\nc\n");
136|    editor.shift(true);
137|    assert_eq!(editor.text(), "a\nb\nc\n");
138|}
139|
140|#[test]
141|fn line_column_is_one_based_and_follows_the_cursor() {
142|    let mut lines = make("one\ntwo\nthree\n");
143|    lines.set_cursor(0, false);
144|    assert_eq!(lines.line_column(), (1, 1));
145|    lines.set_cursor(5, false);
146|    assert_eq!(lines.line_column(), (2, 2));
147|    lines.set_cursor(12, false);
148|    assert_eq!(lines.line_column(), (3, 5));
149|}
150|
151|#[test]
152|fn movement_covers_characters_words_lines_and_document() {
153|    let text = "alpha beta\ngamma\n";
154|    let mut editor = make(text);
155|    editor.set_cursor(0, false);
156|    editor.move_by(Direction::WordRight, false, None);
157|    assert_eq!(editor.cursor(), 6);
158|    editor.move_by(Direction::WordRight, false, None);
159|    assert_eq!(editor.cursor(), 10);
160|    editor.move_by(Direction::WordLeft, false, None);
161|    assert_eq!(editor.cursor(), 6);
162|    editor.move_by(Direction::LineEnd, false, None);
163|    assert_eq!(editor.cursor(), 10);
164|    editor.move_by(Direction::LineStart, false, None);
165|    assert_eq!(editor.cursor(), 0);
166|    editor.move_by(Direction::LineDown, false, None);
167|    assert_eq!(editor.line_column(), (2, 1));
168|    editor.move_by(Direction::LineUp, false, None);
169|    assert_eq!(editor.line_column(), (1, 1));
170|    editor.move_by(Direction::DocumentEnd, false, None);
171|    assert_eq!(editor.cursor(), text.len() - 1);
172|    editor.move_by(Direction::DocumentStart, false, None);
173|    assert_eq!(editor.cursor(), 0);
174|}
175|
176|#[test]
177|fn vertical_movement_clamps_to_a_shorter_line() {
178|    let mut editor = make("long line here\nab\n");
179|    editor.set_cursor(14, false);
180|    editor.move_by(Direction::LineDown, false, None);
181|    assert_eq!(editor.line_column(), (2, 3), "column clamps to the line end");
182|    editor.move_by(Direction::LineUp, false, None);
183|    assert_eq!(editor.line_column(), (1, 15));
184|}
185|
186|#[test]
187|fn movement_never_splits_a_multibyte_character() {
188|    let mut editor = make("héllo 👋\n");
189|    editor.set_cursor(0, false);
190|    editor.move_by(Direction::Right, false, None);
191|    assert_eq!(editor.cursor(), 1);
192|    editor.move_by(Direction::Right, false, None);
193|    assert_eq!(editor.cursor(), 3);
194|    editor.move_by(Direction::DocumentEnd, false, None);
195|    assert_eq!(editor.text_in(editor.selection().start..editor.cursor()), "héllo 👋");
196|}
197|
198|#[test]
199|fn select_word_and_select_all_cover_the_expected_spans() {
200|    let mut editor = make("alpha beta\n");
201|    editor.select_word_at(7);
202|    assert_eq!(editor.text_in(editor.selection()), "beta");
203|    editor.select_all();
204|    assert_eq!(editor.text_in(editor.selection()), "alpha beta\n");
205|}
206|
207|#[test]
208|fn clamping_keeps_the_cursor_inside_the_buffer() {
209|    let mut editor = make("ab\ncd\n");
210|    editor.set_cursor(999, false);
211|    assert_eq!(editor.line_column(), (2, 3));
212|    editor.move_by(Direction::Right, false, None);
213|    assert_eq!(editor.cursor(), 5, "stays put at the end");
214|    editor.move_by(Direction::LineUp, false, None);
215|    assert_eq!(editor.line_column(), (1, 3));
216|    editor.set_cursor(1, false);
217|    editor.move_by(Direction::LineUp, false, None);
218|    assert_eq!(editor.line_column(), (1, 1));
219|}
220|
221|#[test]
222|fn mark_saved_clears_the_modified_state() {
223|    let mut editor = make("hello\n");
224|    editor.set_cursor(5, false);
225|    editor.insert("!");
226|    assert!(editor.is_dirty());
227|    editor.mark_saved();
228|    assert!(!editor.is_dirty());
229|}
230|
231|#[test]
232|fn a_utf8_document_survives_an_edit_at_its_end() {
233|    let mut editor = make("héllo\n");
234|    let end = editor.text().len() - 1;
235|    editor.set_cursor(end, false);
236|    editor.insert("👋");
237|    assert_eq!(editor.text(), "héllo👋\n");
238|    assert_eq!(editor.line_column(), (1, 7));
239|}
240|
241|#[test]
242|fn revision_advances_on_every_change() {
243|    let mut editor = make("a\n");
244|    let first = editor.revision();
245|    editor.set_cursor(0, false);
246|    editor.insert("b");
247|    assert!(editor.revision() > first);
248|    let second = editor.revision();
249|    editor.undo();
250|    assert!(editor.revision() > second);
251|}
252|
253|#[test]
254|fn a_long_session_drops_the_oldest_undo_steps() {
255|    let mut editor = make("\n");
256|    editor.set_cursor(0, false);
257|    for _ in 0..MAX_HISTORY + 50 {
258|        editor.insert("x");
259|    }
260|    let mut steps = 0;
261|    while editor.can_undo() {
262|        editor.undo();
263|        steps += 1;
264|    }
265|    assert!(steps <= MAX_HISTORY, "history is bounded: {steps}");
266|}
267|
268|#[test]
269|fn decoded_text_feeds_the_editor_without_losing_bytes() {
270|    let (text, format) = decode(b"caf\xe9\r\n").unwrap();
271|    let mut editor = Editor::new(&text, format.clone());
272|    editor.set_cursor(0, false);
273|    editor.insert("> ");
274|    assert_eq!(editor.text(), "> café\r\n");
275|    assert!(format.latin1_fallback);
276|}