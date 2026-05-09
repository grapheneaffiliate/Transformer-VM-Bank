variable "vpc_id"         { type = string }
variable "subnet_id"      { type = string }
variable "ssh_key_name"   { type = string }
variable "scrape_targets" { type = list(string) }

# Observability VM runs the docker-compose stack from ops/ — Prometheus,
# Grafana, Alertmanager, Loki, Promtail, Tempo. Cloud-init renders the
# scrape_targets list into ops/prometheus.yml and starts the stack.

output "prometheus_url" { value = "http://prom.internal:9090" }
output "grafana_url"    { value = "http://grafana.internal:3000" }
