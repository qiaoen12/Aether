ALTER TABLE api_keys
ADD COLUMN IF NOT EXISTS per_ip_concurrency_limit integer;
