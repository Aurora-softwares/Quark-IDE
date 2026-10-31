# Repository instructions

- For every requested change to this OS project, update its current development version,
  completed work, next target, and remaining release gates when the change affects them.
  Use the [Hydrogen — Development project](https://github.com/orgs/Aurora-softwares/projects/4)
  for current progress. A Git tag is historical release evidence, not the source of the
  current development version.
- Every user-facing feature must be listed in `tests/feature-coverage.json` with at
  least two distinct, meaningful tests. Update the register when changing features.
  Run `python3 scripts/check-feature-coverage.py` as well as the Rust checks.
  A passing register does not replace native-platform manual acceptance.
- There should be little to no extra markdown / logs / docs files explaining what things do. That
  should all be andled within the assigned project documentation file. This way there is nothing stray
  to be accidentally forgotten or left un-updated on changes and all of the latest changes are available to everyone.
