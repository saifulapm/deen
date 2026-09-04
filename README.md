# deen

Check your Quran recitation against the text, from the command line.

You pick an ayah, you recite it, and it tells you which words were right —
word by word, not as a score. It runs entirely on your machine: no account, no
upload, and the recording is deleted the moment it has been scored.

```console
$ deen recite 1:5
… recite, then press enter …
{"ayah":{…},"heard":"إِيَّاكَ نَعْبُدُ وَإِيَّاكَ نَسْتَعِينُ","correct":4,"total":4,"accuracy":1.0,…}

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

## What it does and does not check

**It checks that you said the right words, in the right order.** Wrong words,
skipped words and inserted words are caught and located.

**It does not check tajweed.** Articulation errors — ص/س, ض/د, ق/ك, ط/ت — are
exactly what a speech model's language prior repairs on your behalf: it hears
an approximate sound, recognises the word from context, and reports it as
correct. A tool that claimed otherwise would tell you your recitation was fine
precisely when you needed to hear that it was not.

So this is worth using between lessons. It is not a teacher, and it will not
tell you that your madd was short or your ḍād was a dāl.

## Requirements

- [voxtype](https://github.com/hyprsyn/voxtype) — used only for its one-shot
  `transcribe` subcommand; the dictation daemon is never involved.
- A Quran-tuned whisper model in ggml form (see Install).
- `pw-record` and `wpctl` (pipewire-utils) for recording.

## Install

```sh
cargo install --path .
```

Then the data and the model:

```sh
# The text: Uthmani script with Bangla (Muhiuddin Khan) and English (Sahih
# International), built once from alquran.cloud. It never changes, so nothing
# refreshes it and every command works offline.
mkdir -p ~/.local/share/deen
deen-fetch-data > ~/.local/share/deen/quran.json   # see scripts/fetch-data

# The duas: Hisn al-Muslim, from sunnah.com, cross-checked against a pinned
# archive of the same page. Also fetched once.
scripts/fetch-duas > ~/.local/share/deen/duas.json

# The model: Tarteel's Quran fine-tune of whisper-base, q8_0.
curl -fL -o /tmp/q.bin https://huggingface.co/ram-a-dhan/tarteel-whisper-quran-ggml/resolve/main/tarteel-ai-whisper-base-ar-quran-ggml-q8_0.bin
```

**Every published ggml conversion of this model needs a four-byte fix before
whisper.cpp will load it.** The header declares `n_text_ctx = 1024` while
`decoder.positional_embedding` is the standard `448`, so it is rejected with
*"tensor 'decoder.positional_embedding' has wrong size"*. The tensor is fine;
only the header lies. Patch offset 24:

```sh
printf '\xc0\x01\x00\x00' | dd of=/tmp/q.bin bs=1 seek=24 conv=notrunc
mv /tmp/q.bin ~/.local/share/voxtype/models/ggml-quran-base-q8.bin
```

## Configuration

Environment overrides, all optional:

| | |
|---|---|
| `DEEN_DATA` | the Quran JSON (default `~/.local/share/deen/quran.json`) |
| `DEEN_DUAS` | the duas JSON (default `~/.local/share/deen/duas.json`) |
| `DEEN_MODEL` | the ggml model (default `~/.local/share/voxtype/models/ggml-quran-base-q8.bin`) |

`deen` writes its own voxtype config to `~/.cache/deen/whisper.toml` and
regenerates it whenever the model path changes. Your dictation config is never
touched — three things in that generated file are load-bearing and the reasons
are in `src/recite.rs`.

## Stopping a recording

`deen recite <ayah>` records until **stdin closes**, or a line arrives on it,
or `--max-seconds` (default 120) elapses. From a terminal that means pressing
enter. From a program it means closing the pipe — no PIDs, no signals.

Pass `--wav <file>` to score an existing 16 kHz mono recording instead.

## Licence

MIT.
