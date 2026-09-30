import { Router } from "express";

const router = Router();
router.get("/items", (req, res) => { res.json([]); });
router.post("/items", (req, res) => { res.json({}); });
export { router };
