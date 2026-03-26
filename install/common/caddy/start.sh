#!/bin/sh

# If deepwell isn't available yet, or is failing for an unknown reason,
# then use the provisional Caddyfile so at least Komodo is reachable
# and a web server is running.
#
# Note that the caddy health check will return failure during that time.
if nc -z deepwell 2747 && wikijump-generate-caddyfile; then
	echo 'Installing generated Caddyfile...'
	mv /tmp/Caddyfile /etc/caddy/Caddyfile
else
	echo 'Cannot reach DEEPWELL, using provisional Caddyfile to start'
	# Template for the provisional Caddyfile is already installed in the container
	deploy_host="$(jq -r .params.deploy_host /etc/caddy-request.json)"
	sed -i "s/<<DEPLOY_HOST>>/$deploy_host/" /etc/caddy/Caddyfile
fi

# It'll fork off and the child process will be tracked
echo 'Starting cron...'
crond

echo 'Starting caddy...'
exec caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
