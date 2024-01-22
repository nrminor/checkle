# checkle: Extremely fast checksum runner for arbitrarily large batches of files

`checkle` has the following goals:
1. Find all recently transferred files based on a set of file attribute filters
2. Spread generating checksums from a few hashing options across as many (virtual) cores as possible.
3. If a manifest of checksums is provided, spread comparing checksums across cores as well.
4. Have an easy-to-use command line interface.
5. Print a report to `stdout` on which files should be re-transferred.
