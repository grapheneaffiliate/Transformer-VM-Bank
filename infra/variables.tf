variable "region" {
  type        = string
  description = "AWS region for the deployment."
  default     = "us-west-2"
}

variable "vpc_cidr" {
  type        = string
  description = "CIDR block for the deployment VPC."
  default     = "10.42.0.0/16"
}

variable "deployment_id" {
  type        = string
  description = "Short unique tag appended to globally-named resources (S3 buckets, etc)."
}

variable "ssh_key_name" {
  type        = string
  description = "Name of the AWS key-pair the operator uses for SSH."
}

variable "psl_version" {
  type        = string
  description = "PSL release tag to deploy (matches a release in github.com/grapheneaffiliate/Transformer-VM-Bank)."
  default     = "v0.1.0"
}

variable "public_domain" {
  type        = string
  description = "DNS name for the light-client gateway ALB (must be Route 53 zone owned by operator)."
}
