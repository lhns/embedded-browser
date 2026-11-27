"""Generates the synthetic corpus pages: python testpages/generate.py testpages"""
import base64, os, random, struct, sys, zlib

OUT = sys.argv[1]
random.seed(42)


def png(w, h, fn):
    raw = b"".join(b"\x00" + b"".join(bytes(fn(x, y)) for x in range(w)) for y in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def data_uri(b):
    return "data:image/png;base64," + base64.b64encode(b).decode()


# ---------------------------------------------------------------- dashboard
ROOMS = ["Living room", "Kitchen", "Bedroom", "Office", "Bathroom", "Hallway"]
def dashboard():
    cards = []
    for i, room in enumerate(ROOMS):
        on = i % 2 == 0
        t = 19.5 + i * 0.7
        cards.append(f"""
    <section class="card{' on' if on else ''}" id="card-{i}">
      <div class="card-head"><span class="icon bulb"></span><h2>{room}</h2>
        <label class="toggle"><input type="checkbox"{' checked' if on else ''}><span></span></label></div>
      <div class="row"><span class="icon thermo"></span><span class="value" id="temp-{i}">{t:.1f}&nbsp;°C</span><span class="muted">Humidity {40 + i * 3}&nbsp;%</span></div>
      <div class="bar"><div style="width:{30 + i * 11}%"></div></div>
      <div class="row small"><span>Brightness</span><span class="muted">{30 + i * 11}&nbsp;%</span></div>
    </section>""")
    entities = "".join(
        f'<li><span class="dot {c}"></span><span class="name">{n}</span><span class="state">{s}</span></li>'
        for n, s, c in [("Front door", "Locked", "ok"), ("Garage door", "Closed", "ok"),
                        ("Washing machine", "Running · 23 min", "warn"), ("Back window", "Open", "alert"),
                        ("Robot vacuum", "Docked", "ok"), ("Mailbox", "Empty", "ok")])
    bars = "".join(f'<div class="col" style="height:{h}%"><span>{d}</span></div>'
                   for d, h in zip("MTWTFSS", [62, 48, 75, 55, 90, 40, 66]))
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=1024, initial-scale=1">
<title>Home — Overview</title>
<!-- Synthetic Home-Assistant-like dashboard (no real entities). Icons are pure CSS. -->
<style>
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; height: 100%; }}
  body {{ font-family: sans-serif; background: #eef1f5; color: #1c2430; font-size: 14px; }}
  header {{ display: flex; align-items: center; gap: 16px; height: 56px; padding: 0 20px;
           background: #03a9f4; color: white; }}
  header h1 {{ font-size: 20px; font-weight: normal; margin: 0; flex: 1; }}
  header .clock {{ font-size: 18px; }}
  nav {{ display: flex; gap: 4px; padding: 0 20px; background: #039be5; }}
  nav a {{ color: rgba(255,255,255,.8); text-decoration: none; padding: 8px 14px; border-bottom: 3px solid transparent; }}
  nav a.active {{ color: white; border-bottom-color: white; }}
  main {{ display: grid; grid-template-columns: 2fr 1fr; gap: 16px; padding: 16px 20px; }}
  .cards {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }}
  .card {{ background: white; border-radius: 12px; padding: 12px; box-shadow: 0 1px 3px rgba(0,0,0,.15); }}
  .card.on {{ background: linear-gradient(160deg, #fff8e1, #ffffff 60%); }}
  .card-head {{ display: flex; align-items: center; gap: 8px; }}
  .card-head h2 {{ font-size: 15px; margin: 0; flex: 1; }}
  .row {{ display: flex; align-items: center; gap: 8px; margin-top: 10px; }}
  .row.small {{ font-size: 12px; justify-content: space-between; margin-top: 4px; }}
  .value {{ font-size: 22px; font-weight: bold; }}
  .muted {{ color: #6b7686; margin-left: auto; }}
  .bar {{ height: 6px; border-radius: 3px; background: #e3e7ee; margin-top: 10px; overflow: hidden; }}
  .bar div {{ height: 100%; background: #ffb300; border-radius: 3px; }}
  .icon {{ display: inline-block; width: 22px; height: 22px; flex: none; }}
  .icon.bulb {{ border-radius: 50%; background: radial-gradient(circle at 40% 35%, #fff3c4, #ffc107 60%, #ff8f00); }}
  .card:not(.on) .icon.bulb {{ background: radial-gradient(circle at 40% 35%, #f5f5f5, #9e9e9e); }}
  .icon.thermo {{ width: 10px; border-radius: 5px; border: 2px solid #e53935; border-bottom-width: 8px; }}
  .toggle input {{ display: none; }}
  .toggle span {{ display: block; width: 36px; height: 20px; border-radius: 10px; background: #bdbdbd; position: relative; }}
  .toggle span::after {{ content: ""; position: absolute; top: 2px; left: 2px; width: 16px; height: 16px;
                         border-radius: 50%; background: white; }}
  .card.on .toggle span {{ background: #03a9f4; }}
  .card.on .toggle span::after {{ left: 18px; }}
  aside {{ display: flex; flex-direction: column; gap: 12px; }}
  .panel {{ background: white; border-radius: 12px; padding: 12px 14px; box-shadow: 0 1px 3px rgba(0,0,0,.15); }}
  .panel h3 {{ margin: 0 0 8px; font-size: 15px; }}
  .weather {{ display: flex; align-items: center; gap: 14px; }}
  .sun {{ width: 48px; height: 48px; border-radius: 50%; background: #ffca28; box-shadow: 0 0 0 6px #fff1c1; }}
  .temp {{ font-size: 30px; }}
  ul.entities {{ list-style: none; margin: 0; padding: 0; }}
  ul.entities li {{ display: flex; align-items: center; gap: 8px; padding: 5px 0; border-top: 1px solid #eef0f3; }}
  ul.entities li:first-child {{ border-top: 0; }}
  .dot {{ width: 10px; height: 10px; border-radius: 50%; }}
  .dot.ok {{ background: #43a047; }} .dot.warn {{ background: #fb8c00; }} .dot.alert {{ background: #e53935; }}
  .state {{ margin-left: auto; color: #6b7686; }}
  .chart {{ display: flex; align-items: flex-end; gap: 8px; height: 90px; padding-top: 6px; }}
  .chart .col {{ flex: 1; background: #81d4fa; border-radius: 4px 4px 0 0; position: relative; }}
  .chart .col span {{ position: absolute; bottom: -16px; left: 0; right: 0; text-align: center; font-size: 11px; color: #6b7686; }}
</style>
</head>
<body>
<header><h1>Home</h1><span class="clock">14:32</span></header>
<nav><a class="active" href="#">Overview</a><a href="#">Energy</a><a href="#">Cameras</a><a href="#">Climate</a></nav>
<main>
  <div class="cards">{''.join(cards)}
  </div>
  <aside>
    <div class="panel weather"><div class="sun"></div><div><div class="temp">21&nbsp;°C</div><div class="muted">Partly cloudy · wind 12&nbsp;km/h</div></div></div>
    <div class="panel"><h3>Status</h3><ul class="entities" id="status">{entities}</ul></div>
    <div class="panel"><h3>Energy this week (kWh)</h3><div class="chart">{bars}</div></div>
  </aside>
</main>
</body>
</html>
"""


# ---------------------------------------------------------------- article
WORDS = ("river valley harbour council railway station market cathedral bridge district province "
         "century trade wool timber granite festival population census tram archive museum orchard "
         "estuary mill guild charter parish quarry canal lighthouse ferry vineyard dialect chronicle").split()
SENT = [
    "The town of Lindmoor grew up around a ford on the {0}, where drovers crossed on their way to the coast.",
    "By the late {1}th century the {2} had become the busiest in the region, handling {3} tonnes of {4} a year.",
    "Local historians disagree about the origin of the name; the earliest {5} spells it in at least four ways.",
    "A {6} opened in {7}, linking the {8} to the {9} and cutting the journey to the capital to under a day.",
    "During the {10} the {11} was rebuilt in stone, and much of that work survives in the present {12}.",
    "The {13} recorded {14} inhabitants in {15}, a figure that has changed little since the closure of the {16}.",
    "Visitors often note the contrast between the narrow lanes of the old {17} and the wide avenues laid out after the fire.",
    "Each summer the {18} draws crowds from neighbouring towns, and the {19} stays open late for the occasion.",
]
def sentence():
    s = random.choice(SENT)
    nums = {1: lambda: str(random.randint(12, 19)), 3: lambda: f"{random.randint(2, 90) * 1000:,}",
            7: lambda: str(random.randint(1820, 1990)), 14: lambda: f"{random.randint(9, 40) * 1000:,}",
            15: lambda: str(random.randint(1850, 2020))}
    return s.format(*[nums[i]() if i in nums else random.choice(WORDS) for i in range(20)])


def para(n):
    return " ".join(sentence() for _ in range(n))


def article():
    imgs = [data_uri(png(160, 110, lambda x, y, k=k: ((x // 16 * 24 + k * 40) % 256, (y // 16 * 30 + 90) % 256, (120 + k * 50) % 256)))
            for k in range(4)]
    sections = []
    titles = ["Etymology", "History", "Geography", "Climate", "Economy", "Demographics", "Culture", "Transport",
              "Education", "Notable people"]
    for si, t in enumerate(titles):
        body = []
        if si % 3 == 1:
            body.append(f'<figure class="thumb {"tright" if si % 2 else "tleft"}"><img src="{imgs[si % 4]}" width="160" height="110" alt=""><figcaption>{t} of Lindmoor, illustration</figcaption></figure>')
        for _ in range(4):
            body.append(f"<p>{para(6)}</p>")
        if t == "Climate":
            rows = "".join(f"<tr><th>{m}</th><td>{random.randint(2, 24)}</td><td>{random.randint(-4, 14)}</td><td>{random.randint(30, 110)}</td></tr>"
                           for m in ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])
            body.append(f'<table class="wikitable"><caption>Climate data for Lindmoor</caption><tr><th>Month</th><th>High °C</th><th>Low °C</th><th>Rain mm</th></tr>{rows}</table>')
        if t == "Demographics":
            rows = "".join(f"<tr><td>{y}</td><td>{random.randint(9000, 40000):,}</td><td>{random.uniform(-3, 9):+.1f}%</td></tr>" for y in range(1850, 2030, 20))
            body.append(f'<table class="wikitable"><tr><th>Year</th><th>Pop.</th><th>±%</th></tr>{rows}</table>')
        if t in ("Transport", "Notable people"):
            items = "".join(f"<li><a href=\"#\">{random.choice(WORDS).title()} {random.choice(WORDS)}</a> – {sentence()}</li>" for _ in range(8))
            body.append(f"<ul>{items}</ul>")
        if t == "Economy":
            items = "".join(f"<li>{sentence()}</li>" for _ in range(5))
            body.append(f"<ol>{items}</ol>")
        sections.append(f'<h2 id="s{si}">{t}</h2>\n' + "\n".join(body))
    toc = "".join(f'<li><a href="#s{i}"><span class="tocnumber">{i + 1}</span> {t}</a></li>' for i, t in enumerate(titles))
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=1024, initial-scale=1">
<title>Lindmoor — Encyclopedia</title>
<!-- Synthetic encyclopedia-style article about a fictional town; text is generated. -->
<style>
  body {{ margin: 0; font-family: sans-serif; color: #202122; background: #f6f6f6; font-size: 14px; line-height: 1.6; }}
  #top {{ background: white; border-bottom: 1px solid #a7d7f9; padding: 8px 16px; display: flex; gap: 12px; align-items: center; }}
  #top .logo {{ font-family: serif; font-size: 20px; }}
  #top input {{ flex: 1; max-width: 360px; padding: 4px 6px; border: 1px solid #a2a9b1; }}
  #content {{ background: white; margin: 0 0 0 150px; padding: 16px 24px; border-left: 1px solid #a7d7f9; }}
  #side {{ position: absolute; left: 0; top: 50px; width: 140px; padding: 8px; font-size: 12px; }}
  #side a {{ display: block; color: #0645ad; text-decoration: none; padding: 2px 0; }}
  h1 {{ font-family: serif; font-weight: normal; font-size: 28px; border-bottom: 1px solid #a2a9b1; margin: 0 0 4px; }}
  h2 {{ font-family: serif; font-weight: normal; font-size: 21px; border-bottom: 1px solid #a2a9b1; margin: 18px 0 6px; clear: both; }}
  a {{ color: #0645ad; }}
  .infobox {{ float: right; width: 260px; margin: 0 0 12px 16px; border: 1px solid #a2a9b1; background: #f8f9fa; font-size: 12px; padding: 4px; border-collapse: collapse; }}
  .infobox th, .infobox td {{ text-align: left; padding: 2px 4px; vertical-align: top; }}
  .infobox caption {{ font-weight: bold; font-size: 15px; padding: 4px; }}
  .thumb {{ border: 1px solid #c8ccd1; background: #f8f9fa; padding: 3px; margin: 4px 0 8px; width: 168px; font-size: 12px; }}
  .tright {{ float: right; margin-left: 14px; }}
  .tleft {{ float: left; margin-right: 14px; }}
  figure {{ margin: 0; }}
  #toc {{ display: inline-block; border: 1px solid #a2a9b1; background: #f8f9fa; padding: 6px 12px; font-size: 13px; }}
  #toc ul {{ list-style: none; margin: 0; padding: 0; }}
  .tocnumber {{ color: #202122; padding-right: 4px; }}
  table.wikitable {{ border-collapse: collapse; background: #f8f9fa; margin: 8px 0; font-size: 13px; }}
  table.wikitable th, table.wikitable td {{ border: 1px solid #a2a9b1; padding: 2px 8px; }}
  table.wikitable th {{ background: #eaecf0; }}
  #footer {{ margin-left: 150px; padding: 12px 24px; font-size: 11px; color: #54595d; }}
</style>
</head>
<body>
<div id="top"><span class="logo">Encyclopedia</span><form action="#"><input type="search" name="q" placeholder="Search"></form></div>
<div id="side"><a href="#">Main page</a><a href="#">Contents</a><a href="#">Current events</a><a href="#">Random article</a><a href="#">About</a><a href="#">Help</a></div>
<div id="content">
<h1>Lindmoor</h1>
<div class="muted">From the encyclopedia</div>
<table class="infobox"><caption>Lindmoor</caption>
<tr><td colspan="2"><img src="{imgs[0]}" width="250" height="160" alt=""></td></tr>
<tr><th>Country</th><td>Northland</td></tr><tr><th>Region</th><td>Upper Vale</td></tr>
<tr><th>Founded</th><td>c. 1140</td></tr><tr><th>Area</th><td>48.2 km²</td></tr>
<tr><th>Population (2021)</th><td>31,402</td></tr><tr><th>Elevation</th><td>112 m</td></tr>
<tr><th>Time zone</th><td>UTC+1</td></tr></table>
<p><b>Lindmoor</b> is a market town in the Upper Vale. {para(5)}</p>
<p>{para(5)}</p>
<div id="toc"><b>Contents</b><ul>{toc}</ul></div>
{chr(10).join(sections)}
</div>
<div id="footer">Synthetic test page for embedded-browser. Text generated; not a real place.</div>
</body>
</html>
"""


# ---------------------------------------------------------------- link list
def links():
    rows = []
    for i in range(30):
        title = " ".join(random.choice(WORDS) for _ in range(random.randint(4, 9))).capitalize()
        dom = random.choice(["example.com", "example.org", "example.net", "blog.example", "news.example"])
        rows.append(f"""<tr class="athing"><td class="rank">{i + 1}.</td><td class="vote"><a href="#" class="up"></a></td>
<td class="title"><a href="#">{title}</a> <span class="site">({dom})</span></td></tr>
<tr><td colspan="2"></td><td class="sub">{random.randint(3, 900)} points by user{random.randint(1, 999)} {random.randint(1, 23)} hours ago | <a href="#">hide</a> | <a href="#">{random.randint(0, 400)}&nbsp;comments</a></td></tr>
<tr class="spacer"><td colspan="3"></td></tr>""")
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=1024, initial-scale=1">
<title>Link News</title>
<!-- Synthetic link-aggregator page with forms; titles are generated. -->
<style>
  body {{ margin: 0; background: white; font-family: sans-serif; font-size: 13px; }}
  #wrap {{ width: 85%; margin: 8px auto; background: #f6f6ef; }}
  #bar {{ background: #ff6600; padding: 2px 4px; display: flex; align-items: center; gap: 8px; }}
  #bar .logo {{ border: 1px solid white; color: white; font-weight: bold; width: 18px; height: 18px; text-align: center; line-height: 18px; }}
  #bar b {{ color: black; }}
  #bar a {{ color: black; text-decoration: none; }}
  #bar .right {{ margin-left: auto; }}
  table.items {{ border-collapse: collapse; margin: 6px 0; }}
  td {{ padding: 0 2px; }}
  td.rank {{ color: #828282; text-align: right; vertical-align: top; width: 30px; }}
  td.vote a.up {{ display: inline-block; width: 0; height: 0; border-left: 5px solid transparent; border-right: 5px solid transparent;
                 border-bottom: 9px solid #9a9a9a; margin: 3px 2px 0; }}
  td.title a {{ color: black; text-decoration: none; font-size: 14px; }}
  .site, td.sub, td.sub a {{ color: #828282; font-size: 11px; text-decoration: none; }}
  tr.spacer td {{ height: 5px; }}
  form {{ margin: 8px 12px; }}
  fieldset {{ border: 1px solid #c0c0b0; padding: 8px; margin-bottom: 10px; }}
  label {{ display: inline-block; width: 80px; }}
  input[type=text], input[type=password], textarea {{ border: 1px solid #a0a0a0; padding: 2px 4px; font-size: 13px; }}
  textarea {{ width: 420px; height: 70px; }}
  button {{ padding: 2px 10px; }}
  #foot {{ border-top: 2px solid #ff6600; text-align: center; padding: 10px; font-size: 12px; }}
</style>
</head>
<body>
<div id="wrap">
<div id="bar"><span class="logo">L</span><b>Link News</b><a href="#">new</a> | <a href="#">past</a> | <a href="#">ask</a> | <a href="#">submit</a><span class="right"><a href="#">login</a></span></div>
<table class="items">{''.join(rows)}</table>
<form action="#" method="post"><fieldset><legend>Login</legend>
<div><label for="u">username:</label><input type="text" id="u" name="u" size="20"></div>
<div><label for="p">password:</label><input type="password" id="p" name="p" size="20"></div>
<div><input type="checkbox" id="r" name="r" checked> <span>remember me</span> <button type="submit">login</button></div>
</fieldset></form>
<form action="#" method="post"><fieldset><legend>Add comment</legend>
<textarea name="text">Write something kind.</textarea><br>
<select name="sort"><option>newest</option><option>top</option></select> <button type="submit">add comment</button>
</fieldset></form>
<div id="foot"><form action="#">Search: <input type="text" name="q" size="17"></form></div>
</div>
</body>
</html>
"""


for name, fn in [("dashboard.html", dashboard), ("article.html", article), ("links.html", links)]:
    with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
        f.write(fn())
    print(name, os.path.getsize(os.path.join(OUT, name)))
