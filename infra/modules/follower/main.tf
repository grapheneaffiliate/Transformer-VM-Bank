variable "vpc_id"         { type = string }
variable "subnet_id"      { type = string }
variable "ssh_key_name"   { type = string }
variable "sequencer_addr" { type = string }
variable "instance_type"  { type = string, default = "m6i.large" }
variable "psl_version"    { type = string }

# Follower module is intentionally minimal — followers are stateless
# w.r.t. canonical state (they re-derive). Disk only holds local cache.
# Real implementation mirrors sequencer module structure.

output "ssh_endpoint" { value = "follower.example.invalid" }
output "private_dns"  { value = "follower.example.invalid" }
