# Contributing

Thanks for helping. A few ground rules keep this manageable:

- **Open an issue first** for anything larger than a bug fix, so we can agree on the shape.
- **Sign off your commits** (`git commit -s`). By doing so you certify the
  [Developer Certificate of Origin](https://developercertificate.org/) for your contribution.
- **Tests.** Go changes need tests (`go test -p 1 ./...` with `VENGEANCE_TEST_DATABASE_URL` set);
  user-facing changes should keep the Playwright suite in `e2e/` green.
- **API changes** update `docs/api.md` in the same pull request.
- **No new heavy dependencies** without a reason. Vengeance should stay one small binary.

Code is AGPL-3.0; contributions are accepted under the same license.
