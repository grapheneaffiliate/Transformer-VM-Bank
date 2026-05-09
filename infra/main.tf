# Reference top-level Terraform for a single-region PSL deployment.
# See infra/README.md for context.

terraform {
  required_version = ">= 1.6"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 5.0" }
  }
}

provider "aws" {
  region = var.region
}

module "network" {
  source     = "./modules/network"
  cidr_block = var.vpc_cidr
  az_count   = 3
}

module "backup_buckets" {
  source       = "./modules/backup_buckets"
  hot_bucket   = "psl-backups-hot-${var.deployment_id}"
  cold_bucket  = "psl-backups-cold-${var.deployment_id}"
  retention_d  = 90
}

module "sequencer" {
  source              = "./modules/sequencer"
  vpc_id              = module.network.vpc_id
  subnet_id           = module.network.private_subnet_ids[0]
  ssh_key_name        = var.ssh_key_name
  state_volume_size_g = 200
  instance_type       = "m6i.xlarge"
  backup_hot_uri      = "s3://${module.backup_buckets.hot_bucket}/"
  backup_cold_uri     = "s3://${module.backup_buckets.cold_bucket}/"
  psl_version         = var.psl_version
}

module "followers" {
  source        = "./modules/follower"
  count         = 3
  vpc_id        = module.network.vpc_id
  subnet_id     = module.network.private_subnet_ids[count.index]
  ssh_key_name  = var.ssh_key_name
  sequencer_addr = module.sequencer.private_dns
  instance_type = "m6i.large"
  psl_version   = var.psl_version
}

module "light_client_gw" {
  source         = "./modules/light_client_gw"
  vpc_id         = module.network.vpc_id
  public_subnets = module.network.public_subnet_ids
  ssh_key_name   = var.ssh_key_name
  sequencer_addr = module.sequencer.private_dns
  domain         = var.public_domain
  psl_version    = var.psl_version
}

module "observability" {
  source       = "./modules/observability"
  vpc_id       = module.network.vpc_id
  subnet_id    = module.network.private_subnet_ids[0]
  ssh_key_name = var.ssh_key_name
  scrape_targets = concat(
    [module.sequencer.private_dns],
    module.followers[*].private_dns,
    [module.light_client_gw.private_dns],
  )
}
