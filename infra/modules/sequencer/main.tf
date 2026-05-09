variable "vpc_id"              { type = string }
variable "subnet_id"           { type = string }
variable "ssh_key_name"        { type = string }
variable "state_volume_size_g" { type = number, default = 200 }
variable "instance_type"       { type = string, default = "m6i.xlarge" }
variable "backup_hot_uri"      { type = string }
variable "backup_cold_uri"     { type = string }
variable "psl_version"         { type = string }

# Pinned Ubuntu 24.04 AMI lookup (operator-supplied via data source in real use).
data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"]  # Canonical
  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*"]
  }
}

resource "aws_security_group" "sequencer" {
  name   = "psl-sequencer"
  vpc_id = var.vpc_id

  # SSH only from operator's bastion / VPN — not 0.0.0.0/0 in real deploy.
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/8"]
  }
  # Sequencer RPC: followers + light-client gateway only.
  ingress {
    from_port   = 26657
    to_port     = 26657
    protocol    = "tcp"
    self        = false
    cidr_blocks = ["10.42.0.0/16"]
  }
  # Prometheus scrape.
  ingress {
    from_port   = 9100
    to_port     = 9100
    protocol    = "tcp"
    cidr_blocks = ["10.42.0.0/16"]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_ebs_volume" "state" {
  availability_zone = data.aws_subnet.this.availability_zone
  size              = var.state_volume_size_g
  type              = "gp3"
  encrypted         = true
}

data "aws_subnet" "this" { id = var.subnet_id }

resource "aws_instance" "this" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.instance_type
  subnet_id              = var.subnet_id
  vpc_security_group_ids = [aws_security_group.sequencer.id]
  key_name               = var.ssh_key_name
  iam_instance_profile   = aws_iam_instance_profile.sequencer.name

  user_data = templatefile("${path.module}/cloud-init.sh.tftpl", {
    psl_version     = var.psl_version
    backup_hot_uri  = var.backup_hot_uri
    backup_cold_uri = var.backup_cold_uri
  })

  tags = { Name = "psl-sequencer-primary", Role = "sequencer" }
}

resource "aws_volume_attachment" "state" {
  device_name = "/dev/sdf"
  volume_id   = aws_ebs_volume.state.id
  instance_id = aws_instance.this.id
}

# IAM role + policy: instance can write to backup buckets and pushgateway.
resource "aws_iam_role" "sequencer" {
  name = "psl-sequencer"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ec2.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
}

resource "aws_iam_role_policy" "sequencer_backup" {
  role = aws_iam_role.sequencer.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = ["s3:PutObject", "s3:GetObject", "s3:ListBucket"]
      Resource = [var.backup_hot_uri, "${var.backup_hot_uri}*", var.backup_cold_uri, "${var.backup_cold_uri}*"]
    }]
  })
}

resource "aws_iam_instance_profile" "sequencer" {
  name = "psl-sequencer"
  role = aws_iam_role.sequencer.name
}

output "ssh_endpoint" { value = aws_instance.this.private_dns }
output "private_dns"  { value = aws_instance.this.private_dns }
