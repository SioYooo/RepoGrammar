from flask import Flask

app = Flask(__name__)


@app.route("/status")
def status():
    return {"ok": True}


@app.route("/version")
def version():
    return {"version": "1.0"}
