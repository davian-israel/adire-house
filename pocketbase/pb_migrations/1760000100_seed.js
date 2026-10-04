/// <reference path="../pb_data/types.d.ts" />
// Bootstraps the service superuser, the default pricing policy, the first admin
// login and (optionally) demo vendors and artworks.
//
// Env vars:
//   PB_SUPERUSER_EMAIL / PB_SUPERUSER_PASSWORD  service account used by the Rust app
//   ADIRE_ADMIN_EMAIL / ADIRE_ADMIN_PASSWORD    first admin login for /admin
//   ADIRE_SEED_DEMO=true                        load sample vendors and artworks

migrate((app) => {
  const env = (k, d) => $os.getenv(k) || d;

  // Service superuser for the Rust backend
  const suEmail = env("PB_SUPERUSER_EMAIL", "");
  if (suEmail) {
    const su = new Record(app.findCollectionByNameOrId("_superusers"));
    su.set("email", suEmail);
    su.setPassword(env("PB_SUPERUSER_PASSWORD", ""));
    app.save(su);
  }

  // Default pricing policy: 45% markup, shipping lanes in USD cents
  const policy = new Record(app.findCollectionByNameOrId("pricing_policy"));
  policy.set("markup_bps", 4500);
  policy.set("rates", {
    local: { S: 1200, M: 2500, L: 6000 },
    cross: { S: 2400, M: 4800, L: 9500 },
    intl:  { S: 4500, M: 9500, L: 18000 },
  });
  policy.set("fx", { USD: 1, CAD: 1.37, EUR: 0.92, GBP: 0.79 });
  app.save(policy);

  // First admin login
  const users = app.findCollectionByNameOrId("users");
  const adminEmail = env("ADIRE_ADMIN_EMAIL", "");
  if (adminEmail) {
    const admin = new Record(users);
    admin.set("email", adminEmail);
    admin.setPassword(env("ADIRE_ADMIN_PASSWORD", ""));
    admin.setVerified(true);
    admin.set("name", "Adire House Admin");
    admin.set("role", "admin");
    app.save(admin);
  }

  if (env("ADIRE_SEED_DEMO", "") !== "true") return;

  // ---- Demo data (fictional artists and shops) ----
  const V = app.findCollectionByNameOrId("vendors");
  const A = app.findCollectionByNameOrId("artworks");
  const S = app.findCollectionByNameOrId("stock");

  const vendors = {
    efua:   ["Efua Mensah-Darko", "artist", "Ghana", "Accra", "Paints market women and Accra street life in layered acrylic.", "verified"],
    lagos:  ["Lagos Atelier Collective", "shop", "Nigeria", "Lagos", "A Yaba studio representing emerging painters and printmakers.", "verified"],
    wanjiru:["Wanjiru Kamau", "artist", "Kenya", "Nairobi", "Landscapes of the Rift Valley in oil and palette knife.", "verified"],
    teranga:["Maison Téranga", "shop", "Senegal", "Dakar", "Gallery-shop in Plateau working with Dakar textile and glass painters.", "verified"],
    selam:  ["Selam Tesfaye", "artist", "Ethiopia", "Addis Ababa", "Mixed-media work drawing on Ethiopian manuscript colour.", "verified"],
    bamako: ["Bamako Bogolan Studio", "shop", "Mali", "Bamako", "Mud-cloth workshop run by three generations of dyers.", "verified"],
    thandeka:["Thandeka Mokoena", "artist", "South Africa", "Johannesburg", "Ndebele-inspired geometric painting. Awaiting studio visit.", "pending"],
  };
  const ids = {};
  for (const [key, [name, kind, country, city, bio, status]] of Object.entries(vendors)) {
    const r = new Record(V);
    r.set("name", name); r.set("kind", kind); r.set("country", country);
    r.set("city", city); r.set("bio", bio); r.set("status", status);
    app.save(r);
    ids[key] = r.id;
  }

  // Demo vendor login for the vendor portal
  const vendorUser = new Record(users);
  vendorUser.set("email", "efua@demo.adirehouse.local");
  vendorUser.setPassword("demo-vendor-123");
  vendorUser.setVerified(true);
  vendorUser.set("name", "Efua Mensah-Darko");
  vendorUser.set("role", "vendor");
  vendorUser.set("vendor", ids.efua);
  app.save(vendorUser);

  // [vendor, artist, title, medium, materials, dimensions, size, price USD, year, stock location]
  const works = [
    ["efua", "Efua Mensah-Darko", "Makola at Noon", "painting", "Acrylic on canvas", "90 × 120 cm", "L", 620, 2026, "UK"],
    ["efua", "Efua Mensah-Darko", "Kenkey Sellers II", "painting", "Acrylic on canvas", "60 × 75 cm", "M", 380, 2026, "ORIGIN"],
    ["efua", "Efua Mensah-Darko", "Kente Weaver, Bonwire", "painting", "Acrylic and woven strip collage", "70 × 90 cm", "M", 450, 2026, "US"],
    ["lagos", "Tunde Adebayo-Okoro", "Indigo Resist No. 4", "textile", "Hand-dyed adire, framed", "70 × 90 cm", "M", 290, 2025, "US"],
    ["lagos", "Ngozi Ike", "Third Mainland Dusk", "print", "Screenprint, edition of 25", "40 × 50 cm", "S", 120, 2026, "US"],
    ["lagos", "Tunde Adebayo-Okoro", "Olokun Circles", "textile", "Hand-dyed adire on cotton", "100 × 140 cm", "L", 410, 2026, "ORIGIN"],
    ["lagos", "Ngozi Ike", "Danfo Yellow", "print", "Linocut, edition of 15", "50 × 70 cm", "M", 170, 2026, "ORIGIN"],
    ["wanjiru", "Wanjiru Kamau", "Longonot After Rain", "painting", "Oil on linen", "80 × 100 cm", "M", 540, 2026, "CA"],
    ["wanjiru", "Wanjiru Kamau", "Naivasha, Early", "painting", "Oil on board", "30 × 40 cm", "S", 210, 2025, "ORIGIN"],
    ["teranga", "Awa Ndiaye", "Souwère: The Fisherman's Wife", "painting", "Reverse glass painting", "35 × 45 cm", "S", 180, 2026, "UK"],
    ["teranga", "Moussa Faye", "Strip Weave, Saloum", "textile", "Hand-loomed cotton strips", "110 × 160 cm", "L", 460, 2025, "UK"],
    ["selam", "Selam Tesfaye", "Manuscript Red", "mixed_media", "Pigment, gesso and goatskin", "60 × 80 cm", "M", 700, 2026, "ORIGIN"],
    ["selam", "Selam Tesfaye", "Lalibela Window", "mixed_media", "Pigment on hand-made paper", "40 × 55 cm", "S", 330, 2026, "US"],
    ["bamako", "Fatoumata Coulibaly", "Crocodile Marks", "textile", "Bogolan mud cloth", "100 × 150 cm", "L", 240, 2026, "US"],
    ["bamako", "Fatoumata Coulibaly", "Djenné Grid", "textile", "Bogolan mud cloth", "60 × 90 cm", "M", 150, 2026, "CA"],
    ["bamako", "Seydou Traoré", "Niger Bend", "textile", "Bogolan, natural dyes", "80 × 120 cm", "M", 190, 2025, "ORIGIN"],
    ["thandeka", "Thandeka Mokoena", "Mapogo House Wall", "painting", "Acrylic on canvas", "80 × 80 cm", "M", 390, 2026, "ORIGIN"],
  ];
  for (const [v, artist, title, medium, materials, dims, size, usd, year, loc] of works) {
    const a = new Record(A);
    a.set("vendor", ids[v]); a.set("artist", artist); a.set("title", title);
    a.set("medium", medium); a.set("materials", materials); a.set("dimensions", dims);
    a.set("size_class", size); a.set("vendor_price_cents", usd * 100); a.set("year", year);
    a.set("status", "listed");
    app.save(a);
    const s = new Record(S);
    s.set("artwork", a.id); s.set("location", loc); s.set("quantity", 1);
    app.save(s);
  }
}, (app) => {
  // Seed data is not rolled back automatically.
});
