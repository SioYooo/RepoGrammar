! Bounded negative fixture: the exact anchor spelling without the import that
! gives it meaning. Records the blocking identity unknown, never an anchor.
module missing_use
  implicit none

contains

  subroutine test_unproven(error)
    type(error_type), allocatable, intent(out) :: error
  end subroutine test_unproven

end module missing_use
