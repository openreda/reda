# Contributing to REDA

Thank you for helping. REDA is developed in a private repository, `openreda/reda`. Source releases are published from it to [openreda/openreda](https://github.com/openreda/openreda), which is where issues and pull requests from the public go. Until v1.0, the public repository follows the private one, so its history may be squashed or rewritten between source releases. We may adapt an accepted change before it lands in a source release.

## Licence of contributions

REDA's public releases are licensed under the GNU General Public License, version 2 or later (see [LICENSE](LICENSE)). **Contributions are accepted under the MIT License, not the GPL.**

By opening a pull request, or otherwise submitting a change, to openreda/openreda, you agree that:

1. Your contribution is licensed under the MIT License below, with you (or your employer, if it owns your work) as the copyright holder.
2. You have the right to submit it under that licence. It is your own work, or it comes from a source whose licence allows it to be relicensed under MIT, and you say which source.
3. The REDA maintainers may include your contribution both in the GPL releases and in versions of REDA under other licences, including an internal version that is not under the GPL and a possible future MIT-licensed release.

We keep the right to maintain a version of REDA that is free of the GPL. This is not for a closed edition with extra features: the GPL release is our focus, and our end goal is to release REDA under MIT once it is established. The GPL protects REDA from paid forks in the meantime (see [Why GPL now, and MIT later](README.md#why-gpl-now-and-mit-later)). Taking contributions under MIT is what lets us move them to MIT along with the rest of REDA. If you cannot agree to this, please open an issue that describes the change instead of sending code.

Do not submit code copied from GPL or LGPL projects (KiCad, LibrePCB, Horizon EDA and others), or from any source that is not permissively licensed. They are fine as references for ideas, but their code cannot be relicensed under MIT.

```
MIT License

Copyright (c) <year> <contributor>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Sign-off

Confirm the terms above by adding a `Signed-off-by:` line to every commit (`git commit -s`), and by ticking the licence box in the pull request template. A pull request without both cannot be merged.

## Building and testing

See [Building from source](README.md#building-from-source). Run `cargo fmt`, `cargo clippy --workspace` and `cargo test --workspace` before opening a pull request.
