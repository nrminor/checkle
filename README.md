# checkle: Extremely fast checksum runner for arbitrarily large batches of files
[![Open Source Starter Files](https://github.com/nrminor/checkle/actions/workflows/open-source-starter.yml/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/open-source-starter.yml) [![Rust CI](https://github.com/nrminor/checkle/actions/workflows/rust-ci.yml/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/rust-ci.yml)

It checks files so fast it will make you chuckle.

`checkle` has the following goals:
1. Find all recently transferred files based on a set of file attribute filters
2. Spread hashing across as many (virtual) cores as possible.
3. If a manifest of checksums is provided, spread checksums across cores as well.
4. Support md5 for backward compatibility along with a few cryptographically secure hashing functions.
5. Be capable of reaching into `tar` and `zip` archives to checksum files without decompressing the whole archive.
6. Minimize filesystem calls and memory allocations to maximize performance.
7. Have an easy-to-use command line interface.
8. Be easy to install.
9. Print a report to `stdout` on which files should be re-transferred.
10. Accept standard input to fit in with unix pipeline.
