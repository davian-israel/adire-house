/// <reference path="../pb_data/types.d.ts" />
// Adire House schema. Every collection is locked to superusers (rules = null):
// the Rust service is the only client, so PocketBase is never exposed to browsers.

migrate((app) => {
  const autodates = [
    { type: "autodate", name: "created", onCreate: true, onUpdate: false },
    { type: "autodate", name: "updated", onCreate: true, onUpdate: true },
  ];

  // ---- Pricing context: one settings record holds the pricing policy ----
  const settings = new Collection({
    type: "base",
    name: "pricing_policy",
    fields: [
      { type: "number", name: "markup_bps", required: true, onlyInt: true, min: 0, max: 100000 },
      // { "local": {"S":1200,"M":2500,"L":6000}, "cross": {...}, "intl": {...} } in USD cents
      { type: "json", name: "rates", required: true, maxSize: 20000 },
      // { "USD": 1, "CAD": 1.37, "EUR": 0.92, "GBP": 0.79 }
      { type: "json", name: "fx", required: true, maxSize: 20000 },
      ...autodates,
    ],
  });
  app.save(settings);

  // ---- Vendors context ----
  const vendors = new Collection({
    type: "base",
    name: "vendors",
    fields: [
      { type: "text", name: "name", required: true, max: 120 },
      { type: "select", name: "kind", required: true, maxSelect: 1, values: ["artist", "shop"] },
      { type: "text", name: "country", required: true, max: 80 },
      { type: "text", name: "city", required: true, max: 80 },
      { type: "text", name: "bio", max: 1000 },
      { type: "select", name: "status", required: true, maxSelect: 1, values: ["pending", "verified", "suspended"] },
      ...autodates,
    ],
    indexes: ["CREATE INDEX idx_vendors_status ON vendors (status)"],
  });
  app.save(vendors);

  // ---- Identity: extend the built-in users auth collection with a role ----
  const users = app.findCollectionByNameOrId("users");
  users.fields.add(new SelectField({ name: "role", required: true, maxSelect: 1, values: ["admin", "vendor"] }));
  users.fields.add(new RelationField({ name: "vendor", collectionId: vendors.id, maxSelect: 1, cascadeDelete: false }));
  users.listRule = null; users.viewRule = null; users.createRule = null; users.updateRule = null; users.deleteRule = null;
  app.save(users);

  // ---- Catalogue context ----
  const artworks = new Collection({
    type: "base",
    name: "artworks",
    fields: [
      { type: "relation", name: "vendor", required: true, collectionId: vendors.id, maxSelect: 1, cascadeDelete: false },
      { type: "text", name: "artist", required: true, max: 120 },
      { type: "text", name: "title", required: true, max: 120 },
      { type: "select", name: "medium", required: true, maxSelect: 1, values: ["painting", "textile", "print", "sculpture", "mixed_media"] },
      { type: "text", name: "materials", max: 160 },
      { type: "text", name: "dimensions", max: 60 },
      { type: "select", name: "size_class", required: true, maxSelect: 1, values: ["S", "M", "L"] },
      { type: "number", name: "vendor_price_cents", required: true, onlyInt: true, min: 100 },
      { type: "number", name: "year", onlyInt: true },
      { type: "select", name: "status", required: true, maxSelect: 1, values: ["listed", "withdrawn"] },
      { type: "file", name: "image", maxSelect: 1, maxSize: 8388608, mimeTypes: ["image/jpeg", "image/png", "image/webp"] },
      ...autodates,
    ],
    indexes: [
      "CREATE INDEX idx_artworks_vendor ON artworks (vendor)",
      "CREATE INDEX idx_artworks_status ON artworks (status)",
    ],
  });
  app.save(artworks);

  // ---- Inventory context: where each piece physically is, and how many ----
  const stock = new Collection({
    type: "base",
    name: "stock",
    fields: [
      { type: "relation", name: "artwork", required: true, collectionId: artworks.id, maxSelect: 1, cascadeDelete: true },
      { type: "select", name: "location", required: true, maxSelect: 1, values: ["ORIGIN", "US", "UK", "CA"] },
      { type: "number", name: "quantity", onlyInt: true, min: 0 },
      ...autodates,
    ],
    indexes: ["CREATE UNIQUE INDEX idx_stock_artwork ON stock (artwork)"],
  });
  app.save(stock);

  // ---- Customers: favourites keyed by an anonymous customer key cookie ----
  const favourites = new Collection({
    type: "base",
    name: "favourites",
    fields: [
      { type: "text", name: "customer_key", required: true, max: 64 },
      { type: "select", name: "kind", required: true, maxSelect: 1, values: ["vendor", "artist"] },
      { type: "text", name: "target", required: true, max: 120 },
      ...autodates,
    ],
    indexes: ["CREATE UNIQUE INDEX idx_fav_unique ON favourites (customer_key, kind, target)"],
  });
  app.save(favourites);

  // ---- Orders / Escrow context ----
  const orders = new Collection({
    type: "base",
    name: "orders",
    fields: [
      { type: "text", name: "number", required: true, max: 20 },
      { type: "relation", name: "artwork", required: true, collectionId: artworks.id, maxSelect: 1, cascadeDelete: false },
      { type: "relation", name: "vendor", required: true, collectionId: vendors.id, maxSelect: 1, cascadeDelete: false },
      { type: "text", name: "buyer_name", required: true, max: 120 },
      { type: "email", name: "buyer_email", required: true },
      { type: "text", name: "shipping_address", required: true, max: 500 },
      { type: "select", name: "destination", required: true, maxSelect: 1, values: ["US", "CA", "EU", "UK"] },
      // Price snapshot at the time of purchase (USD cents) so later policy changes never alter an order
      { type: "number", name: "vendor_price_cents", required: true, onlyInt: true },
      { type: "number", name: "markup_cents", required: true, onlyInt: true },
      { type: "number", name: "shipping_cents", required: true, onlyInt: true },
      { type: "number", name: "total_cents", required: true, onlyInt: true },
      { type: "text", name: "lane", required: true, max: 10 },
      { type: "text", name: "currency", required: true, max: 3 },
      { type: "number", name: "fx_rate", required: true },
      { type: "select", name: "status", required: true, maxSelect: 1,
        values: ["awaiting_payment", "paid_held", "shipped", "delivered_released", "refunded", "cancelled"] },
      { type: "text", name: "payment_ref", max: 120 },
      { type: "text", name: "tracking", max: 120 },
      ...autodates,
    ],
    indexes: [
      "CREATE UNIQUE INDEX idx_orders_number ON orders (number)",
      "CREATE INDEX idx_orders_status ON orders (status)",
    ],
  });
  app.save(orders);
}, (app) => {
  for (const name of ["orders", "favourites", "stock", "artworks", "pricing_policy"]) {
    try { app.delete(app.findCollectionByNameOrId(name)); } catch (_) {}
  }
  const users = app.findCollectionByNameOrId("users");
  users.fields.removeByName("vendor");
  users.fields.removeByName("role");
  app.save(users);
  try { app.delete(app.findCollectionByNameOrId("vendors")); } catch (_) {}
});
