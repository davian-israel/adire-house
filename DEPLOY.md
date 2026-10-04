# Deploying to Railway

Adire House runs as **one Railway service**: a single container holding the Rust app (public) and PocketBase
(localhost only), with the database on a Railway volume. Build config lives in `railway.toml` and
`Dockerfile.single`; `deploy/start.sh` starts PocketBase, waits for it, then starts the app.

Keep it at **one replica**. PocketBase is SQLite on a volume, so it cannot be shared between instances.

## First deploy

1. In Railway, choose **New Project → Deploy from GitHub repo** and pick this repository.
   Railway reads `railway.toml` and builds `Dockerfile.single`. The first build takes about 5 minutes (Rust release build).
2. On the service, open **Settings → Volumes → Add volume** and mount it at **`/data`**.
3. Open **Variables** and add:

   | Variable | Value |
   |---|---|
   | `SESSION_SECRET` | 64+ random characters (`openssl rand -hex 32`) |
   | `PB_SUPERUSER_EMAIL` | service account email, e.g. `service@adirehouse.app` |
   | `PB_SUPERUSER_PASSWORD` | long random password |
   | `ADIRE_ADMIN_EMAIL` | your admin login for `/admin` |
   | `ADIRE_ADMIN_PASSWORD` | your admin password |
   | `ADIRE_SEED_DEMO` | `true` to load the fictional demo catalogue, otherwise leave unset |

   Railway provides `PORT` automatically. `SECURE_COOKIES=true` and the PocketBase settings are already in the image.
4. Open **Settings → Networking → Generate Domain** (or add your own domain), then redeploy.
5. Check `https://<your-domain>/healthz` returns `ok`, then sign in at `/login`.

The `ADIRE_*` and `PB_SUPERUSER_*` variables are read by the seed migration **on the first start only**,
when the volume is empty. Changing them later does not change existing accounts. Change passwords in the
PocketBase admin UI instead (see below).

## Updates

Every push to `main` triggers a new build and deploy. Schema changes go in a new file under
`pocketbase/pb_migrations/`; PocketBase applies it on the next start.

## PocketBase admin UI

PocketBase listens on localhost inside the container, so it is not reachable from the internet. When you
need its dashboard:

1. Add the variable `PB_HTTP=0.0.0.0:8090` and add a second domain on the service that targets port **8090**.
2. After the redeploy, open `https://<that-domain>/_/` and sign in with `PB_SUPERUSER_EMAIL`.
3. When you're done, delete the domain and the `PB_HTTP` variable, and redeploy.

## Backups

Railway volumes support backups in the service's volume settings. Schedule daily backups before you take real
orders, and test a restore once.
