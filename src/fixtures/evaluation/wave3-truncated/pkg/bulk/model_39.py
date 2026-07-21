from pydantic import BaseModel


class Item39(BaseModel):
    id: int
    label: str
    quantity: int
