variable "vpc_id"         { type = string }
variable "public_subnets" { type = list(string) }
variable "ssh_key_name"   { type = string }
variable "sequencer_addr" { type = string }
variable "domain"         { type = string }
variable "psl_version"    { type = string }

# Light-client gateway sits behind a public ALB with TLS termination
# at AWS Certificate Manager. Real implementation provisions:
#   - aws_lb (application, internet-facing)
#   - aws_lb_listener (443/HTTPS, ACM cert for var.domain)
#   - aws_lb_target_group → instance running psl-light-client-gateway
#   - aws_route53_record (alias var.domain → ALB)

output "public_url"  { value = "https://${var.domain}" }
output "private_dns" { value = "lcgw.example.invalid" }
