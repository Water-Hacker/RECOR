# Top-level Terraform inputs.

variable "environment" {
  description = "Deployment environment: dev, staging, or prod"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "environment must be one of dev, staging, prod."
  }
}

variable "aws_region" {
  description = "AWS region; ignored when running on GCP / on-prem"
  type        = string
  default     = "eu-west-1"
}

variable "kubernetes_host" {
  description = "Kubernetes API server URL"
  type        = string
}

variable "kubernetes_ca_pem" {
  description = "Base64-decoded Kubernetes cluster CA certificate"
  type        = string
  sensitive   = true
}

variable "kubernetes_token" {
  description = "Service-account token for terraform-bot"
  type        = string
  sensitive   = true
}

variable "cluster_name" {
  description = "Name of the Kubernetes cluster RÉCOR runs on"
  type        = string
  default     = "recor"
}
