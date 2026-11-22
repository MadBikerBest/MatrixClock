MatrixClockGlyphs.otf — подмножество шрифта Noto Sans JP Medium (Google, SIL OFL 1.1, см. OFL.txt):
ASCII (U+0020–007E), Latin-1 и Latin Extended-A (U+00A0–017F, что есть в исходнике:
немецкие и французские буквы, «°»), русская кириллица (U+0401, U+0410–044F, U+0451)
и полуширинная катакана (U+FF61–FF9F).
Источник: https://github.com/notofonts/noto-cjk (Sans/SubsetOTF/JP/NotoSansJP-Medium.otf).
Нарезано: pyftsubset --unicodes='U+0020-007E,U+00A0-017F,U+0401,U+0410-044F,U+0451,U+FF61-FF9F' --name-IDs='*'; семейство переименовано в "MatrixClock Glyphs",
чтобы не перекрывать полный Noto Sans JP. Встраивается в бинарник (графический режим) и ставится в
~/.local/share/fonts (текстовый режим в X-терминалах).

MatrixClockText.ttf — подмножество шрифта Russo One (Jovanny Lemonad, SIL OFL 1.1, см. OFL-Russo.txt):
ASCII, Latin-1 и Latin Extended-A (U+00A0–017F), русская кириллица. Надписи (погода, дата) к угловатым
цифрам (digits.style = "angular"). Встраивается в бинарник.
Источник: https://github.com/google/fonts (ofl/russoone/RussoOne-Regular.ttf).
Нарезано: pyftsubset --unicodes='U+0020-007E,U+00A0-017F,U+0401,U+0410-044F,U+0451' --name-IDs='*';
семейство переименовано в "MatrixClock Text": у Russo One зарезервированное имя "Russo" (Reserved Font
Name), изменённая версия (подмножество) его носить не может.
