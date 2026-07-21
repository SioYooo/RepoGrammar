from sqlalchemy.orm import Session


class UserRepository:
    def create_user(self, session: Session):
        session.commit()

    def update_user(self, session: Session):
        session.commit()

    def delete_user(self, session: Session):
        session.commit()
