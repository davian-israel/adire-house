# Adire House

A marketplace connecting verified African artists and shops with buyers in the US, Canada, the UK and the EU.
Vendors enter their own price; the platform adds a 45% markup plus lane-based shipping automatically, holds
the buyer's payment in escrow and releases it to the vendor on delivery.

**Stack:** Rust (Axum 0.8, Askama templates) · htmx 2 · PocketBase 0.40 as the database.

```
browser ──htmx──▶ Rust service (Axum) ──REST, superuser token──▶ PocketBase (SQLite)
                   renders HTML                                   all collection rules locked
```

PocketBase is never exposed to browsers. Every collection rule is `null` (superuser only), and the Rust
service is the only client. Artwork photos are streamed through `/media/...` by the service.

## Run it locally

```bash
# 1. PocketBase (download v0.40.0 for your OS from github.com/pocketbase/pocketbase/releases)
cp .env.example .env            # then fill in passwords and SESSION_SECRET (openssl rand -hex 32)
set -a; source .env; set +a
cd pocketbase && ./pocketbase serve --http=127.0.0.1:8090   # runs the migrations on first start

# 2. The app, in another terminal
cargo run
# open http://127.0.0.1:3000
```

Or with Docker: `cp .env.example .env`, fill it in, then `docker compose up --build`.

To deploy to Railway, see [DEPLOY.md](DEPLOY.md).

With `ADIRE_SEED_DEMO=true`, the first start loads 7 fictional vendors and 17 artworks, plus:

| Login | Where |
|---|---|
| `ADIRE_ADMIN_EMAIL` / `ADIRE_ADMIN_PASSWORD` | `/admin` |
| `efua@demo.adirehouse.local` / `demo-vendor-123` | `/vendor` (demo artist) |

Buyers don't sign in. Favourites are tied to an anonymous cookie.

## Domain design

Each bounded context is a module with a pure `domain.rs` (entities, value objects and rules, unit-tested)
and a `repo.rs` (a repository trait plus its PocketBase adapter). `state.rs` is the composition root.

| Context | Module | Owns | Key rules |
|---|---|---|---|
| Catalogue | `src/catalogue` | Artworks, medium, size class, listing status | Title, artist, $1–$1M price; withdraw and relist |
| Vendors | `src/vendors` | Artists and shops | Every vendor starts *pending*; only *verified* vendors are visible; approve and suspend transitions |
| Pricing | `src/pricing` | The pricing policy | `total = vendor price + markup (bps) + rate[lane][size]`; lane routing from stock location to buyer region; FX at display |
| Inventory | `src/inventory` | Stock location (studio, US, UK, CA warehouse) and quantity | Can't oversell; reserve on order, release on cancel or refund |
| Orders / Escrow | `src/orders` | Order aggregate, price snapshot, payment port | `awaiting_payment → paid_held → shipped → delivered_released`, plus refund and cancel; funds move only on those transitions |
| Customers | `src/customers` | Favourite shops and artists | Toggle per anonymous customer key |

Cross-context work lives in application services and read models:

- `src/orders/app.rs`: placing an order checks Catalogue and Vendors, reserves Inventory, quotes with Pricing, then
  calls the `PaymentGateway`. Each step compensates if a later one fails (PocketBase has no multi-collection
  transaction over REST).
- `src/storefront.rs`: the buyer's read model (listing + vendor + stock + quote). Text, medium, country, shop and
  artist filters run in PocketBase; price and delivery-speed filters run after pricing.

### Shipping lanes

| Lane | When | Default S / M / L (USD) |
|---|---|---|
| Local stock | Piece is in a warehouse in the buyer's country | 12 / 25 / 60 |
| Regional stock | UK stock to the EU; US and Canada to each other | 24 / 48 / 95 |
| Ships from Africa | Everything else | 45 / 95 / 180 |

The markup, rates and exchange rates are edited on `/admin` and take effect immediately. Orders keep the
price they were placed at.

## htmx patterns used

- Filters: `hx-get` on the filter form swaps `#results` and pushes the URL; the full page works without JavaScript.
- Artwork detail: cards load into `#modal`; the same URL renders a full page when opened directly.
- Favourites: the toggle swaps itself and updates the Saved count with `hx-swap-oob`.
- Vendor price preview: `hx-post /vendor/quote` re-renders the per-region price table as the vendor types.
- Admin rows (approve, suspend, ship, refund) swap just their own `<tr>`.
- Errors: domain errors return 422 with `HX-Retarget: #flash`, so messages appear in place.

## Before going live

1. **Payments.** Replace `DevGateway` in `src/orders/payments.rs` with a real adapter. Stripe Connect with
   separate charges and transfers fits escrow: charge at checkout, transfer to the vendor on
   `DeliveredReleased`. For payouts to Ghana, Nigeria and Kenya, Paystack or Flutterwave. Handle the provider's
   webhook to call `Order::mark_paid`.
2. **Buyer order links.** `/orders/{id}` is a capability URL. Add a signed token or buyer accounts before
   real use, and email the link on purchase.
3. **Stock races.** `reserve` is read-modify-write. For high-demand drops, enable PocketBase's batch API or
   move the reservation into a PocketBase JS hook so it is atomic.
4. **Search scale.** The storefront loads the filtered set and sorts in memory, which suits a few thousand
   listings. Beyond that, store a computed `buyer_total_usd` per region or add a search index.
5. **Compliance.** Export permits for artworks (Ghana and Nigeria regulate antiquities), EU import VAT (IOSS)
   and UK VAT at checkout, and terms covering returns and damage claims.
6. Set `SECURE_COOKIES=true` behind HTTPS and keep the PocketBase port private.

## Tests

`cargo test` covers the pricing formula and lane routing, money formatting, vendor verification, stock
reservation and the escrow state machine.
