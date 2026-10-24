# deen

The Quran, duas, hadith and a memorisation deck, from the command line.

The Quran, duas and hadith, and a memorisation deck you grade yourself. It
runs entirely on your machine: no account, no upload. `deen recite` and its
microphone code were removed.

```console
$ deen api ayah 1:6 | jq -r .ayah.ar
ٱهْدِنَا ٱلصِّرَٰطَ ٱلْمُسْتَقِيمَ
```

Every command answers with exactly one JSON object on stdout and puts
diagnostics on stderr, so a caller can parse stdout blind. That is what lets a
GUI be a front end for this binary and nothing else.

It also carries the day's adhkar, because a tool for rebuilding a practice is
not only about the Quran:

```console
$ deen dua show 27 | jq -r '.dua | .tr, .en, .ref'
Allāhumma bā'id baynī wa bayna khatāyāya…
O Allah, separate me from my sins as You have separated the East from the West…
Al-Bukhari 1/181, Muslim 1/419.
```

**Every dua comes with the reference the book prints beneath it, and nothing
here writes religious content.** The 268 duas in 132 chapters are Hisn
al-Muslim (Sa'id bin Ali bin Wahf Al-Qahtani); `scripts/fetch-duas` installs
them only if two independently fetched copies of the page agree field for
field, and refuses to install anything if they do not.

And it searches the hadith corpus — ten collections, 36,512 narrations, in
Arabic, English and Bangla, entirely offline:

```console
$ deen hadith search "seeking forgiveness" --limit 1 | jq -r '.hits[0] | .collection, .url'
abudawud
https://sunnah.com/abudawud:3098
```

Every word of the query has to appear; quote it to demand the exact phrase. The
script you type in picks the language. **Grades are kept under the name of the
scholar who gave them** — the graders disagree, and a verdict without its author
is an opinion pretending to be a fact — and every result carries the sunnah.com
URL that settles it.

And it answers the day, for a landing page to draw:

```console
$ deen api daily --date 2026-09-05 | jq -r '"\(.ayah.s):\(.ayah.a)  \(.hadith.collection):\(.hadith.hadith.n)"'
11:9  muslim:5465
```

The pair is a pure function of the date, so every machine shows the same one —
selection, never generation. Pass your LOCAL date; without it the day is UTC's,
which turns over at a strange hour east of Greenwich. The daily narration is
drawn from Bukhari and Muslim only: search reaches the whole corpus because you
went looking, but one that arrives unasked should not be one you have to weigh a
grading dispute about.

## Install

```sh
cargo install --path .
```

Then the data:

```sh
# The text: Uthmani script with Bangla (Muhiuddin Khan) and English (Sahih
# International), built once from alquran.cloud. It never changes, so nothing
# refreshes it and every command works offline.
mkdir -p ~/.local/share/deen
deen-fetch-data > ~/.local/share/deen/quran.json   # see scripts/fetch-data

# The duas: Hisn al-Muslim, from sunnah.com, cross-checked against a pinned
# archive of the same page. Also fetched once.
scripts/fetch-duas > ~/.local/share/deen/duas.json

# The hadith corpus: ten collections in three languages, from a pinned commit
# of fawazahmed0/hadith-api (Unlicense). 104 MB, a few minutes, once.
scripts/fetch-hadith ~/.local/share/deen/hadith
```

## Configuration

Environment overrides, all optional:

| | |
|---|---|
| `DEEN_DATA` | the Quran JSON (default `~/.local/share/deen/quran.json`) |
| `DEEN_DUAS` | the duas JSON (default `~/.local/share/deen/duas.json`) |
| `DEEN_HADITH` | the hadith corpus directory (default `~/.local/share/deen/hadith`) |

## Licence

MIT.
