# Third-party source attribution

This crate ports the following pinned implementations. Source anchors in its Rust documentation identify the corresponding upstream functions.

- **go-yaml v2.4.3** (`go.yaml.in/yaml/v2`): reader, scanner, parser, resolver, and generic decoding. Copyright 2011–2016 Canonical Ltd and contributors; Apache License 2.0 in [LICENSE-APACHE](LICENSE-APACHE), with upstream [NOTICE](NOTICE). Its reader, scanner, and parser descend from libyaml; Copyright 2006 Kirill Simonov, under the MIT license in [LICENSE.libyaml](LICENSE.libyaml).
- **Go 1.26.5 standard library**: string quoting and printable Unicode character data used by `stages.rs` and `stages/quote_print.rs`. Copyright 2009 The Go Authors, under the BSD 3-Clause license in [LICENSE-GO](LICENSE-GO). The printable ranges are generated from this pinned release's `strconv.IsPrint`.
- **Sprig v3.3.0** (`github.com/Masterminds/sprig/v3`): `indent`, `nindent`, `trim`, and `quote` stages. Copyright 2013–2020 Masterminds, under the MIT license in [LICENSE-SPRIG](LICENSE-SPRIG).
- **Helm v4.2.3** (`helm.sh/helm/v4`): manifest splitting and trimming, and the literal `<no value>` replacement. Copyright The Helm Authors, under the Apache License 2.0 in [LICENSE-HELM](LICENSE-HELM).

The Go differential harness calls these pinned packages directly. Its staged go-yaml observation adapter is additive: it leaves every verified upstream source file unchanged.
