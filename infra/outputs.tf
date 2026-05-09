output "sequencer_ssh"        { value = module.sequencer.ssh_endpoint }
output "follower_ssh"         { value = module.followers[*].ssh_endpoint }
output "light_client_gw_url"  { value = module.light_client_gw.public_url }
output "prometheus_url"       { value = module.observability.prometheus_url }
output "grafana_url"          { value = module.observability.grafana_url }
output "backup_hot_uri"       { value = module.backup_buckets.hot_uri }
output "backup_cold_uri"      { value = module.backup_buckets.cold_uri }
