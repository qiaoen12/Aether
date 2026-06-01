ALTER TABLE api_keys
ADD COLUMN per_ip_concurrency_limit INT NULL AFTER concurrent_limit;
