# Crabscape AWS access

Account: `719048844874`. Region: `us-east-1`.

The website is hosted by GitHub Pages at `crabscape.com`; Route 53 hosts its DNS.
The matching/report server currently stays on Fly.io.

`AWS domain configuration` applies `pages-dns.json` on main or manual dispatch.
It uses GitHub OIDC, not stored AWS access keys. Repository variables `AWS_ROLE_ARN`
and `AWS_REGION` identify the role and region. Its trust and permissions are recorded
in `github-trust.json` and `github-permissions.json`. Those two files document the
bootstrapped IAM configuration; changing them does not automatically update IAM.

The role trusts only `StevenKauwe/crabscape` on `refs/heads/main` and can manage
only this hosted zone. Extend permissions for concrete server resources when adding
the AWS server deployment. Do not grant the role account-wide administrator access.

For local maintenance, use `aws login --profile crabscape --region us-east-1`,
then pass `--profile crabscape` to AWS commands. Prefer a named administrator
identity for subsequent sessions; the initial bootstrap used a temporary root login.
No AWS credentials belong in this repository or GitHub secrets.
