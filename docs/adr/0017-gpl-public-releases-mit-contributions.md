# Public releases are GPL-2.0-or-later; contributions come in under MIT

REDA is developed in the private repository `openreda/reda`, and until v1.0 source releases are published from it to the public repository `openreda/openreda`. Those public releases, source and binaries, are licensed `GPL-2.0-or-later`. Pull requests to the public repository are accepted only under the MIT License (inbound MIT, outbound GPL), confirmed by a sign-off on every commit and a checkbox in the pull request template. MIT is the end goal. The GPL is a temporary protection: during early, active development, before REDA is established, AI tools make it quick to take an MIT project, extend it and sell it as a paid product, and the GPL makes anyone who distributes a fork publish its source. Because the copyright holder owns the in-house code and contributions arrive under MIT, an internal version of REDA that is not under the GPL can be kept, so the whole project, contributions included, can move to MIT once REDA is established, without asking every contributor. The internal version is not meant to become a closed edition with extra features; the GPL release is where the work goes. This supersedes ADR-0002's opening statement that REDA is MIT-licensed. ADR-0002's dependency policy is unchanged, and it now carries more weight: a single GPL or LGPL dependency would make a non-GPL version impossible.

## Consequences

- `"or later"` matters: GPL-2.0-only is incompatible with Apache-2.0, which many dependencies use alone or as one choice of a dual licence. Recipients may take the GPL-3.0 terms, which are compatible.
- MIT requires its notice to stay with the code, so contributors' copyright notices must be kept in every version that carries their changes, the internal one included.
- Designs made with REDA are not covered by the GPL. The Starter Library (`library/`) stays MIT, with its own `library/LICENSE`, because GPL terms on symbols, footprints and parts would raise questions about the boards built from them, the same concern that keeps third-party library data out of REDA (ADR-0002).
- Releases published under MIT before this decision (v0.1.0 and earlier) stay MIT for whoever received them.

## Considered Options

- **Stay MIT.** Simplest, and where REDA wants to end up, but anyone could build a paid, closed fork on REDA while it is still taking shape and before it is established.
- **GPL for good.** Protects REDA permanently, but rules out the MIT end goal.
- **GPL inbound and outbound.** Standard, but a contribution under GPL could not be carried into a non-GPL version without the contributor's later permission.
- **A Contributor Licence Agreement.** Gives the same freedom as inbound MIT, but is heavier for contributors and needs legal review. Inbound MIT is a licence contributors already know.
