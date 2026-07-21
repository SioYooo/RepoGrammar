from pydantic import BaseModel


class Item65(BaseModel):
    id: int
    label: str
    quantity: int
