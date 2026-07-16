from flask import Flask

app = Flask(__name__)


@app.route("/only")
def only():
    return {"ok": True}
