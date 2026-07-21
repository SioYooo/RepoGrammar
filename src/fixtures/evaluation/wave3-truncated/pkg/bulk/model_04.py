from pydantic import BaseModel


class Item04(BaseModel):
    id: int
    label: str
    quantity: int
