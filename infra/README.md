# PSL infra-as-code (reference)

This directory contains a **reference** Terraform layout for a small
production deployment of PSL: one sequencer + three followers + one
light-client gateway + the observability stack from `ops/`.

It is intentionally minimal:
- AWS as the example cloud (the modules are written so swapping to
  GCP or Azure is structural, not architectural).
- Single region. Multi-region is a v0.2 concern (see ADR-0002 for
  why we are not in multi-region until BFT consensus lands).
- No managed-Kubernetes complexity. PSL processes run on plain VMs
  under systemd; the observability stack runs as the Docker Compose
  bundle in `ops/docker-compose.yml`. This is deliberate — we want
  the fewest moving parts a real auditor can reason about.

## Structure

```
infra/
├── README.md                  # this file
├── main.tf                    # top-level: stitches modules together
├── variables.tf               # the few inputs (region, cidr, ssh key, etc.)
├── outputs.tf                 # endpoints + IP addresses needed by ops
├── backend.tf.example         # remote state config (rename + edit)
└── modules/
    ├── network/               # VPC, subnets, security groups
    ├── sequencer/             # primary sequencer VM + state EBS volume
    ├── follower/              # follower VM(s)
    ├── light_client_gw/       # light-client gateway VM + ALB
    ├── observability/         # VM running ops/docker-compose.yml
    └── backup_buckets/        # hot + cold S3 buckets for tools/backup.sh
```

## Status

The Terraform here is documentation-quality, not click-deploy-ready
out of the box for an arbitrary AWS account. It encodes the shape
of a real deployment so an auditor or a partner-ops team can read
how PSL is meant to be deployed without reverse-engineering it from
the runbooks.

## Apply (after editing variables)

```bash
cd infra/
cp backend.tf.example backend.tf      # then edit S3 bucket, dynamodb table
terraform init
terraform plan                        # review
terraform apply
```

Outputs include the SSH endpoints, the light-client gateway DNS
name, and the Prometheus/Grafana URLs.

## Why infra-as-code is in this repo at all

For an audit-ready posture, "we have a deployment" is not enough —
the deployment has to be **reproducible from the same git tree as
the binary**. If a partner asks "can you redeploy this exactly", the
answer is `terraform apply`, not "Bob set it up by hand and Bob is
on holiday."

This is the same discipline we apply to the build (`REPRODUCE.md`,
pinned toolchain) and to the data layer (`tools/backup.sh` writing
manifests). The infra layer gets the same treatment.
