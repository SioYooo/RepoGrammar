from flask import Flask

app = Flask(__name__)


@app.route("/health")
def health():
    return {"ok": True}


@app.route("/users")
def users():
    return []


@app.route("/teams")
def teams():
    return []
