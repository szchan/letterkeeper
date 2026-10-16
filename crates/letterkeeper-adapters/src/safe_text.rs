// SPDX-License-Identifier: GPL-3.0-only
/// Deliberately destructive: no Markdown/HTML punctuation, clickable scheme,
/// terminal controls or bidi controls survive. This is not an HTML parser.
pub fn inert_text(input: &str, max: usize) -> String {
    input.chars().filter(|c| (!c.is_control() || *c=='\n') && !matches!(*c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')).map(|c|match c {
 '<'=>'‹','>'=>'›','['=>'［',']'=>'］','('=>'（',')'=>'）','!'=>'！','`'=>'ˋ','*'=>'＊','_'=>'＿','#'=>'＃',':'=>'：','/'=>'／','\\'=>'＼','@'=>'＠','.'=>'．', _=>c }).take(max).collect()
}
